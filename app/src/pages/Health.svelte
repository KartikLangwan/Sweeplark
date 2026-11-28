<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Ring from "../components/Ring.svelte";
  import { api } from "../lib/api";
  import { bytes, pct, uptime } from "../lib/format";
  import { toast , t } from "../lib/state.svelte";
  import type { Health } from "../lib/types";

  let h = $state<Health | null>(null);
  let eco = $state<Record<string, boolean>>({});

  onMount(() => {
    api.health(true).then((x) => (h = x)).catch((e) => toast(String(e), "warn"));
    // Light refresh from the shared live monitor; the full snapshot is read once.
    const t = setInterval(() => api.live().then((l) => { if (h) h = { ...h, memory_used: l.ram_used, cpu_percent: l.cpu_percent, swap_used: l.swap_used }; }).catch(() => {}), 4000);
    return () => clearInterval(t);
  });

  const ram = $derived(h ? (h.memory_used / h.memory_total) * 100 : 0);
  // Only a real CPU sensor is labelled "CPU"; motherboard sensors keep their own label.
  const cpuTemp = $derived(h?.temperatures.find((x) => x.label === "CPU") ?? null);
  const otherTemps = $derived(h ? h.temperatures.filter((x) => x.label !== "CPU").slice(0, 2) : []);
  const wear = $derived.by(() => {
    const b = h?.battery;
    if (!b?.design_capacity_mwh || !b.full_charge_capacity_mwh) return null;
    return Math.max(0, Math.min(100, (1 - b.full_charge_capacity_mwh / b.design_capacity_mwh) * 100));
  });

  async function toggleEco(name: string) {
    const on = !eco[name];
    try {
      const n = await api.efficiency(name, on);
      eco[name] = on;
      toast(on ? t(`${name}: efficiency mode on (${n} processes). Lasts until the app closes.`, `${name}: efficiency mode chalu (${n} process). App band hone tak rahega.`) : t(`${name}: normal mode`, `${name}: normal mode`));
    } catch (e) {
      toast(String(e), "warn");
    }
  }
</script>

<div class="page">
<header class="head"><div><h1>{t("PC health", "PC ki sehat")}</h1><p class="muted">{h ? `${h.host_name} · ${h.os} · uptime ${uptime(h.uptime_secs)}` : "…"}</p></div></header>

{#if h}
  <section class="gauges">
    <div class="card g"><Ring value={h.cpu_percent} label={pct(h.cpu_percent)} sub="CPU" tone={h.cpu_percent > 85 ? "amber" : "teal"} />
      <div class="gtext">
        <strong>{h.cpu_brand}</strong>
        <span class="muted small">{h.cpu_cores} threads · {h.process_count} processes</span>
        {#if cpuTemp}<span class="small num">{t("CPU temperature", "CPU temperature")}: {Math.round(cpuTemp.celsius)}°C</span>{/if}
        {#each otherTemps as tp}<span class="muted small num">{tp.label}: {Math.round(tp.celsius)}°C</span>{/each}
        {#if !cpuTemp}
          <p class="note small">{t("Windows doesn't report the CPU temperature itself. For an exact reading, run LibreHardwareMonitor; Sweeplark will read it.", "Windows khud CPU temperature nahi batata. Sahi reading ke liye LibreHardwareMonitor chalayein; Sweeplark use padh lega.")}</p>
        {/if}
      </div></div>
    <div class="card g"><Ring value={ram} label={pct(ram)} sub="RAM" tone={ram > 90 ? "red" : ram > 80 ? "amber" : "teal"} />
      <div class="gtext">
        <strong class="num">{bytes(h.memory_used)} / {bytes(h.memory_total)}</strong>
        <span class="muted small">Page file: {bytes(h.swap_used)} / {bytes(h.swap_total)}</span>
        {#each h.gpus as g}<span class="muted small">GPU: {g.name}</span>{/each}
      </div></div>
  </section>

  <section class="two">
    <div class="card">
      <div class="card-title"><Icon name="ram" /><h3>{t("Apps using the most RAM", "Sabse zyada RAM lene wali apps")}</h3></div>
      <div class="list">
        {#each h.top_memory.slice(0, 7) as a}
          <div class="item">
            <div class="col grow" style="gap:4px">
              <div class="row"><strong>{a.name}</strong><span class="muted small">{a.processes} {t("processes", "process")}</span>{#if a.system}<span class="chip">Windows</span>{/if}</div>
              <div class="bar"><span style="width:{Math.min(100, (a.memory_bytes / h.memory_total) * 100 * 2.5)}%"></span></div>
            </div>
            <span class="num">{bytes(a.memory_bytes)}</span>
            {#if !a.system}
              <button class="btn small" class:primary={eco[a.name]} onclick={() => toggleEco(a.name)} title={t("Windows Efficiency mode: lower CPU priority and power use", "Windows Efficiency mode: kam CPU priority aur bijli ki bachat")}>
                <Icon name="leaf" size={15} />{eco[a.name] ? "Eco on" : "Eco"}</button>
            {/if}
          </div>
        {/each}
      </div>
      <p class="muted small note-foot">{t("Eco uses Windows Efficiency mode: the app keeps running but takes less CPU in the background. No fake RAM boost.", "Eco Windows ka Efficiency mode hai: app band nahi hoti, bas background mein kam CPU leti hai. Fake RAM boost hum nahi karte.")}</p>
    </div>

    <div class="side">
      <div class="card">
        <div class="card-title"><Icon name="disk" /><h3>{t("Drives", "Drives")}</h3></div>
        {#each h.disks.filter((d) => !d.removable) as d}
          {@const used = (1 - d.free_bytes / d.total_bytes) * 100}
          <div class="drive">
            <div class="row"><strong>{d.mount.replace("\\", "")}</strong><span class="chip">{d.kind}</span>{#if d.is_system}<span class="chip teal">Windows</span>{/if}
              <span class="spacer"></span><span class="num small">{t(`${bytes(d.free_bytes)} free of ${bytes(d.total_bytes)}`, `${bytes(d.free_bytes)} khaali / ${bytes(d.total_bytes)}`)}</span></div>
            <div class="bar" class:amber={used > 85} class:red={used > 92}><span style="width:{used}%"></span></div>
          </div>
        {/each}
        {#each h.physical_disks as p}
          <p class="small row phys"><Icon name={p.health === "Healthy" ? "check" : "alert"} size={15} />{p.name}: <strong class:bad={p.health !== "Healthy"}>{p.health === "Healthy" ? t("Healthy", "Sehatmand") : p.health}</strong></p>
        {/each}
      </div>

      <div class="card">
        <div class="card-title"><Icon name="shield" /><h3>{t("Security", "Suraksha")}</h3><span class="spacer"></span><button class="btn small" onclick={() => api.openSecurity()}>Windows Security</button></div>
        {#if h.security}
          <div class="kvs">
            <div class="kv"><span>Microsoft Defender</span><strong class:bad={h.security.defender_antivirus_enabled === false}>{h.security.defender_antivirus_enabled === false ? t("Off", "Band") : t("On", "Chalu")}</strong></div>
            <div class="kv"><span>{t("Real-time protection", "Real-time suraksha")}</span><strong class:bad={h.security.defender_realtime_enabled === false}>{h.security.defender_realtime_enabled === false ? t("Off", "Band") : t("On", "Chalu")}</strong></div>
            <div class="kv"><span>{t("Virus definitions", "Virus jaankari")}</span><strong>{t(`${h.security.defender_signature_age_days ?? "?"} days old`, `${h.security.defender_signature_age_days ?? "?"} din purani`)}</strong></div>
            {#each h.security.firewall_profiles as [n, ok]}<div class="kv"><span>Firewall ({n})</span><strong class:bad={!ok}>{ok ? t("On", "Chalu") : t("Off", "Band")}</strong></div>{/each}
          </div>
        {:else}<p class="muted small">{t("Not available (another antivirus may be installed).", "Jaankari nahi mili (shayad koi doosra antivirus hai).")}</p>{/if}
      </div>

      {#if h.battery}
        <div class="card">
          <div class="card-title"><Icon name="battery" /><h3>Battery</h3></div>
          <div class="kvs">
            <div class="kv"><span>Charge</span><strong>{h.battery.percent ?? "?"}% {h.battery.charging ? t("· charging", "· charge ho rahi") : ""}</strong></div>
            {#if wear != null}<div class="kv"><span>{t("Wear", "Ghisawat")}</span><strong class:bad={wear >= 40}>{Math.round(wear)}%</strong></div>{/if}
          </div>
          {#if wear != null}
            <div class="bar wear" class:amber={wear >= 20} class:red={wear >= 40}><span style="width:{100 - wear}%"></span></div>
            <p class="muted small">{t(`Holds ${Math.round(100 - wear)}% of its original charge.`, `Nayi battery ke muqable ab ${Math.round(100 - wear)}% charge pakadti hai.`)}</p>
          {/if}
        </div>
      {/if}
    </div>
  </section>
{:else}
  <div class="row"><div class="spin"></div><span class="muted">{t("Checking health…", "Sehat check ho rahi hai…")}</span></div>
{/if}
</div>

<style>
  .page { max-width: 1080px; margin: 0 auto; }
  .head { display: flex; align-items: flex-end; justify-content: space-between; margin-bottom: 18px; }
  .gauges { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin-bottom: 14px; }
  .g { display: flex; align-items: center; gap: 20px; }
  .gtext { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .gtext strong { overflow-wrap: anywhere; }
  .note { margin-top: 8px; padding: 8px 10px; border-radius: 9px; background: var(--surface-2); border: 1px solid var(--line); color: var(--slate); line-height: 1.4; }
  .two { display: grid; grid-template-columns: 1.3fr 1fr; gap: 14px; align-items: start; }
  .side { display: flex; flex-direction: column; gap: 14px; }
  .grow { flex: 1; min-width: 0; }
  .note-foot { margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--line); }
  .drive { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
  .phys { margin: 4px 0 0; }
  .kvs { display: flex; flex-direction: column; }
  .kv { display: flex; justify-content: space-between; gap: 12px; padding: 7px 0; border-top: 1px solid var(--line); font-size: 13.5px; }
  .kv:first-child { border-top: 0; }
  .wear { margin-top: 12px; }
  .bad { color: var(--red-text); }
  @media (max-width: 1050px) { .two, .gauges { grid-template-columns: 1fr; } }
</style>
