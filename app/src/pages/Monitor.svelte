<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { api } from "../lib/api";
  import { bytes } from "../lib/format";
  import { t, toast } from "../lib/state.svelte";
  import type { LiveStats, ProcessGroup } from "../lib/types";

  const N = 60;
  let s = $state<LiveStats | null>(null);
  let hist = $state<Record<string, number[]>>({ cpu: [], gpu: [], ram: [], down: [], up: [] });
  let tab = $state<"live" | "processes">("live");
  let procs = $state<ProcessGroup[]>([]);
  let filter = $state("");
  let eco = $state<Record<string, boolean>>({});

  function push(k: string, v: number) {
    const a = [...(hist[k] ?? []), v];
    hist[k] = a.length > N ? a.slice(a.length - N) : a;
  }

  async function tick() {
    try {
      const x = await api.live();
      s = x;
      push("cpu", x.cpu_percent);
      push("gpu", x.gpu_percent ?? 0);
      push("ram", (x.ram_used / Math.max(1, x.ram_total)) * 100);
      push("down", x.net_down_bps);
      push("up", x.net_up_bps);
    } catch { /* ignore */ }
    if (tab === "processes") procs = await api.processes().catch(() => procs);
  }

  onMount(() => {
    tick();
    const id = setInterval(tick, 1500);
    return () => clearInterval(id);
  });

  function path(values: number[], max: number, w = 300, h = 64) {
    if (values.length < 2) return "";
    const step = w / (N - 1);
    const off = (N - values.length) * step;
    return values.map((v, i) => `${i ? "L" : "M"}${(off + i * step).toFixed(1)},${(h - (Math.min(v, max) / max) * (h - 4) - 2).toFixed(1)}`).join(" ");
  }
  const rate = (b: number) => `${bytes(b)}/s`;
  const netMax = $derived(Math.max(256 * 1024, ...(hist.down ?? []), ...(hist.up ?? [])));
  const shown = $derived(procs.filter((p) => p.name.toLowerCase().includes(filter.toLowerCase())).slice(0, 60));

  async function end(p: ProcessGroup) {
    try { const n = await api.endProcess(p.name); toast(t(`Ended ${p.name} (${n})`, `${p.name} band kiya (${n})`)); procs = await api.processes(); }
    catch (e) { toast(String(e), "warn"); }
  }
  async function toggleEco(p: ProcessGroup) {
    const on = !eco[p.name];
    try { await api.efficiency(p.name, on); eco[p.name] = on; toast(on ? t(`${p.name}: Eco mode on`, `${p.name}: Eco mode chalu`) : t(`${p.name}: normal`, `${p.name}: normal`)); }
    catch (e) { toast(String(e), "warn"); }
  }
</script>

<header class="head">
  <div><h1>{t("Monitor", "Monitor")}</h1><p class="muted">{t("Live CPU, GPU, memory, network and disk. Updates every 1.5 s while this page is open.", "Live CPU, GPU, memory, network aur disk. Yeh page khula ho tab har 1.5 s update.")}</p></div>
  <div class="seg"><button class:on={tab === "live"} onclick={() => (tab = "live")}>{t("Live", "Live")}</button>
    <button class:on={tab === "processes"} onclick={() => { tab = "processes"; tick(); }}>{t("Processes", "Processes")}</button></div>
</header>

{#if tab === "live"}
  {#if s}
    <section class="grid2">
      <div class="card m">
        <div class="row"><span class="ic"><Icon name="cpu" size={17} /></span><strong>CPU</strong><span class="spacer"></span>
          <span class="big num">{Math.round(s.cpu_percent)}%</span></div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.cpu, 100)} class="line" /></svg>
        <div class="row small muted">{(s.cpu_mhz / 1000).toFixed(2)} GHz{#if s.cpu_temp != null} · {Math.round(s.cpu_temp)}°C{/if} · {s.processes} processes</div>
        <div class="cores">{#each s.per_core as c}<span title="{Math.round(c)}%"><i style="height:{Math.max(4, c)}%"></i></span>{/each}</div>
      </div>
      <div class="card m">
        <div class="row"><span class="ic"><Icon name="pulse" size={17} /></span><strong>GPU</strong><span class="muted small ell">{s.gpu_name ?? ""}</span><span class="spacer"></span>
          <span class="big num">{s.gpu_percent != null ? `${Math.round(s.gpu_percent)}%` : "–"}</span></div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.gpu, 100)} class="line g" /></svg>
        <div class="row small muted">
          {#if s.gpu_temp != null}<span class="temp" class:hot={s.gpu_temp >= 80}>{Math.round(s.gpu_temp)}°C</span> ·{/if}
          VRAM {s.vram_used != null ? bytes(s.vram_used) : "–"}{s.vram_total ? ` / ${bytes(s.vram_total)}` : ""}
          {#if s.gpu_temp == null}<span>· {t("temperature: NVIDIA only", "temperature: sirf NVIDIA")}</span>{/if}
        </div>
      </div>
      <div class="card m">
        <div class="row"><span class="ic"><Icon name="ram" size={17} /></span><strong>{t("Memory", "Memory")}</strong><span class="spacer"></span>
          <span class="big num">{Math.round((s.ram_used / s.ram_total) * 100)}%</span></div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.ram, 100)} class="line r" /></svg>
        <div class="row small muted">{bytes(s.ram_used)} / {bytes(s.ram_total)} · {t("page file", "page file")} {bytes(s.swap_used)}</div>
      </div>
      <div class="card m">
        <div class="row"><span class="ic"><Icon name="download" size={17} /></span><strong>{t("Network", "Network")}</strong><span class="spacer"></span>
          <span class="num small">↓ {rate(s.net_down_bps)} · ↑ {rate(s.net_up_bps)}</span></div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.down, netMax)} class="line" /><path d={path(hist.up, netMax)} class="line a" /></svg>
        <div class="row small muted">{t("Disk", "Disk")}: {s.disk_read_bps != null ? `${t("read", "read")} ${rate(s.disk_read_bps)} · ${t("write", "write")} ${rate(s.disk_write_bps ?? 0)}` : "–"}</div>
      </div>
    </section>
  {:else}
    <div class="row"><div class="spin"></div></div>
  {/if}
{:else}
  <div class="card list">
    <div class="item head-row"><input type="text" placeholder={t("Filter apps…", "Apps filter karein…")} bind:value={filter} style="flex:1" />
      <span class="muted small">{procs.length} apps</span></div>
    <div class="item th"><span class="grow">{t("App", "App")}</span><span class="w">CPU</span><span class="w">RAM</span><span class="actions"></span></div>
    {#each shown as p (p.name)}
      <div class="item">
        <div class="col grow" style="gap:1px"><strong>{p.name}</strong><span class="muted small">{p.pids.length} {t("processes", "process")}{p.system ? " · Windows" : ""}</span></div>
        <span class="w num">{p.cpu_percent.toFixed(1)}%</span>
        <span class="w num">{bytes(p.memory_bytes)}</span>
        <span class="actions">
          {#if !p.system}
            <button class="btn small" class:primary={eco[p.name]} title={t("Windows Efficiency mode", "Windows Efficiency mode")} onclick={() => toggleEco(p)}><Icon name="leaf" size={14} />Eco</button>
            <button class="btn small danger" onclick={() => end(p)}>{t("End", "Band")}</button>
          {/if}
        </span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: var(--surface-3); }
  .seg button { border: 0; background: transparent; padding: 6px 16px; border-radius: 999px; cursor: pointer; font-weight: 560; color: var(--slate); }
  .seg button.on { background: var(--surface); color: var(--teal-text); box-shadow: var(--shadow); }
  .grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .m { display: flex; flex-direction: column; gap: 10px; }
  .ic { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 8px; background: var(--teal-soft); color: var(--teal-text); }
  .big { font-size: 22px; font-weight: 650; }
  .ell { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 220px; }
  svg { width: 100%; height: 64px; background: var(--surface-2); border-radius: 8px; }
  .line { fill: none; stroke: var(--teal); stroke-width: 2; vector-effect: non-scaling-stroke; }
  .line.g { stroke: #7c6cf2; }
  .line.r { stroke: var(--amber); }
  .line.a { stroke: var(--mint); stroke-dasharray: 4 3; }
  .cores { display: flex; gap: 3px; height: 34px; align-items: flex-end; }
  .cores span { flex: 1; height: 100%; background: var(--surface-3); border-radius: 3px; display: flex; align-items: flex-end; overflow: hidden; }
  .cores i { display: block; width: 100%; background: var(--teal); border-radius: 3px; }
  .temp.hot { color: var(--red-text); font-weight: 600; }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .w { width: 90px; text-align: right; }
  .actions { width: 150px; display: flex; gap: 6px; justify-content: flex-end; }
  .th { font-size: 12px; color: var(--slate); font-weight: 600; }
  @media (max-width: 1000px) { .grid2 { grid-template-columns: 1fr; } }
</style>
