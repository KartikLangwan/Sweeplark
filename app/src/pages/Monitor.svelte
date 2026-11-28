<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
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
  let sortBy = $state<"cpu" | "ram">("cpu");
  let eco = $state<Record<string, boolean>>({});
  let ending = $state<ProcessGroup | null>(null);

  const CPU_TEMP_HINT = $derived(t("Windows doesn't report the CPU temperature itself. Run LibreHardwareMonitor for an exact reading; Sweeplark reads it automatically.", "Windows khud CPU temperature nahi batata. Sahi reading ke liye LibreHardwareMonitor chalayein; Sweeplark use khud padh lega."));
  const BOARD_HINT = $derived(t("ACPI thermal zone (motherboard), not the CPU itself", "ACPI thermal zone (motherboard), CPU khud nahi"));

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
  const shown = $derived(
    procs
      .filter((p) => p.name.toLowerCase().includes(filter.toLowerCase()))
      .sort((a, b) => (sortBy === "cpu" ? b.cpu_percent - a.cpu_percent : b.memory_bytes - a.memory_bytes))
      .slice(0, 60)
  );

  async function end(p: ProcessGroup) {
    ending = null;
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
  <div class="seg">
    <button class:on={tab === "live"} onclick={() => (tab = "live")}>{t("Live", "Live")}</button>
    <button class:on={tab === "processes"} onclick={() => { tab = "processes"; tick(); }}>{t("Processes", "Processes")}</button>
  </div>
</header>

{#if tab === "live"}
  {#if s}
    <section class="grid2">
      <div class="card m">
        <div class="card-head">
          <span class="ic"><Icon name="cpu" size={17} /></span><strong>CPU</strong>
          <span class="spacer"></span><span class="big num">{Math.round(s.cpu_percent)}%</span>
        </div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.cpu, 100)} class="line" /></svg>
        <div class="cores" title={t("Each bar is one core", "Har bar ek core")}>{#each s.per_core as c}<span title="{Math.round(c)}%"><i style="height:{Math.max(4, c)}%"></i></span>{/each}</div>
        <div class="stats">
          <div><em>{t("Speed", "Speed")}</em><b class="num">{(s.cpu_mhz / 1000).toFixed(2)} GHz</b></div>
          <div title={s.cpu_temp == null ? CPU_TEMP_HINT : undefined}>
            <em>{t("Temperature", "Temperature")}</em><b class="num">{s.cpu_temp != null ? `CPU ${Math.round(s.cpu_temp)}°C` : "CPU —"}</b>
          </div>
          {#if s.board_temp != null}
            <div title={BOARD_HINT}><em>{t("Motherboard", "Motherboard")}</em><b class="num">{Math.round(s.board_temp)}°C</b></div>
          {/if}
          <div><em>{t("Processes", "Processes")}</em><b class="num">{s.processes}</b></div>
        </div>
      </div>

      <div class="card m">
        <div class="card-head">
          <span class="ic"><Icon name="pulse" size={17} /></span><strong>GPU</strong>
          <span class="muted small ell">{s.gpu_name ?? t("No GPU detected", "Koi GPU nahi mila")}</span>
          <span class="spacer"></span><span class="big num">{s.gpu_percent != null ? `${Math.round(s.gpu_percent)}%` : "–"}</span>
        </div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.gpu, 100)} class="line g" /></svg>
        <div class="stats">
          <div title={s.gpu_temp == null ? t("Temperature is read for NVIDIA cards only", "Temperature sirf NVIDIA cards par padha jata hai") : undefined}>
            <em>{t("Temperature", "Temperature")}</em>
            <b class="num" class:hot={s.gpu_temp != null && s.gpu_temp >= 80}>{s.gpu_temp != null ? `${Math.round(s.gpu_temp)}°C` : "—"}</b>
          </div>
          <div><em>VRAM</em><b class="num">{s.vram_used != null ? bytes(s.vram_used) : "–"}{s.vram_total ? ` / ${bytes(s.vram_total)}` : ""}</b></div>
        </div>
      </div>

      <div class="card m">
        <div class="card-head">
          <span class="ic"><Icon name="ram" size={17} /></span><strong>{t("Memory", "Memory")}</strong>
          <span class="spacer"></span><span class="big num">{Math.round((s.ram_used / s.ram_total) * 100)}%</span>
        </div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.ram, 100)} class="line r" /></svg>
        <div class="stats">
          <div><em>{t("In use", "Use ho raha")}</em><b class="num">{bytes(s.ram_used)} / {bytes(s.ram_total)}</b></div>
          <div><em>{t("Page file", "Page file")}</em><b class="num">{bytes(s.swap_used)}</b></div>
        </div>
      </div>

      <div class="card m">
        <div class="card-head">
          <span class="ic"><Icon name="download" size={17} /></span><strong>{t("Network", "Network")}</strong>
          <span class="spacer"></span>
        </div>
        <svg viewBox="0 0 300 64" preserveAspectRatio="none"><path d={path(hist.down, netMax)} class="line" /><path d={path(hist.up, netMax)} class="line a" /></svg>
        <div class="stats">
          <div><em>↓ {t("Download", "Download")}</em><b class="num">{rate(s.net_down_bps)}</b></div>
          <div><em>↑ {t("Upload", "Upload")}</em><b class="num">{rate(s.net_up_bps)}</b></div>
          <div><em>{t("Disk read", "Disk read")}</em><b class="num">{s.disk_read_bps != null ? rate(s.disk_read_bps) : "–"}</b></div>
          <div><em>{t("Disk write", "Disk write")}</em><b class="num">{s.disk_write_bps != null ? rate(s.disk_write_bps) : "–"}</b></div>
        </div>
      </div>
    </section>
  {:else}
    <div class="loading"><div class="spin"></div></div>
  {/if}
{:else}
  <div class="card procs">
    <div class="toolbar">
      <input type="text" class="search" placeholder={t("Filter apps…", "Apps filter karein…")} bind:value={filter} />
      <span class="muted small num">{procs.length} {t("apps", "apps")}</span>
    </div>
    <div class="th">
      <span class="grow">{t("App", "App")}</span>
      <button class="sort" class:on={sortBy === "cpu"} onclick={() => (sortBy = "cpu")}>CPU{sortBy === "cpu" ? " ↓" : ""}</button>
      <button class="sort" class:on={sortBy === "ram"} onclick={() => (sortBy = "ram")}>RAM{sortBy === "ram" ? " ↓" : ""}</button>
      <span class="actions"></span>
    </div>
    {#if !procs.length}
      <div class="loading"><div class="spin"></div></div>
    {:else}
      <div class="list">
        {#each shown as p (p.name)}
          <div class="item">
            <div class="col grow" style="gap:1px"><strong>{p.name}</strong><span class="muted small">{p.pids.length} {t("processes", "process")}{p.system ? ` · ${t("Windows", "Windows")}` : ""}</span></div>
            <span class="w num">{p.cpu_percent.toFixed(1)}%</span>
            <span class="w num">{bytes(p.memory_bytes)}</span>
            <span class="actions">
              {#if !p.system}
                <button class="btn small" class:primary={eco[p.name]} title={t("Windows Efficiency mode", "Windows Efficiency mode")} onclick={() => toggleEco(p)}><Icon name="leaf" size={14} />Eco</button>
                <button class="btn small danger" onclick={() => (ending = p)}>{t("End", "Band")}</button>
              {/if}
            </span>
          </div>
        {:else}
          <div class="empty">{t("No app matches that filter.", "Is filter se koi app nahi mili.")}</div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<Modal open={ending !== null} onclose={() => (ending = null)} title={t("End this app?", "Yeh app band karein?")}>
  {#if ending}<p><strong>{ending.name}</strong> · {ending.pids.length} {t("processes", "process")}</p>
    <p class="muted small" style="margin-top:6px">{t("Unsaved work in it will be lost. Prefer closing it normally if you can.", "Isme unsaved kaam chala jayega. Ho sake toh normal tareeke se band karein.")}</p>{/if}
  {#snippet footer()}<button class="btn" onclick={() => (ending = null)}>{t("Cancel", "Ruko")}</button><button class="btn solid-danger" onclick={() => ending && end(ending)}>{t("End app", "Band karo")}</button>{/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 20px; }
  .head p { margin-top: 4px; }
  .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: var(--surface-3); flex: none; }
  .seg button { border: 0; background: transparent; padding: 6px 16px; border-radius: 999px; cursor: pointer; font-weight: 560; color: var(--slate); }
  .seg button.on { background: var(--surface); color: var(--teal-text); box-shadow: var(--shadow); }

  .grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .m { display: flex; flex-direction: column; gap: 12px; padding: 18px; }
  .card-head { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .card-head strong { white-space: nowrap; }
  .ic { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 8px; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .big { font-size: 22px; font-weight: 650; }
  .ell { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }

  svg { width: 100%; height: 64px; background: var(--surface-2); border-radius: 10px; }
  .line { fill: none; stroke: var(--teal); stroke-width: 2; vector-effect: non-scaling-stroke; }
  .line.g { stroke: #7c6cf2; }
  .line.r { stroke: var(--amber); }
  .line.a { stroke: var(--mint); stroke-dasharray: 4 3; }

  .cores { display: flex; gap: 3px; height: 30px; align-items: flex-end; }
  .cores span { flex: 1; height: 100%; background: var(--surface-3); border-radius: 3px; display: flex; align-items: flex-end; overflow: hidden; }
  .cores i { display: block; width: 100%; background: var(--teal); border-radius: 3px; }

  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 10px 14px; padding-top: 12px; border-top: 1px solid var(--line); margin-top: auto; }
  .stats em { display: block; font-style: normal; font-size: 11.5px; color: var(--slate); margin-bottom: 1px; }
  .stats b { font-size: 13.5px; font-weight: 600; color: var(--ink); white-space: nowrap; }
  .stats b.hot { color: var(--red-text); }

  .loading { display: grid; place-items: center; padding: 48px 0; }

  .procs { padding: 14px 16px 10px; }
  .toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding-bottom: 12px; }
  .search { flex: 1; max-width: 360px; }
  .th { display: flex; align-items: center; gap: 12px; padding: 8px 4px; font-size: 12px; color: var(--slate); font-weight: 600; border-bottom: 1px solid var(--line); }
  .sort { width: 90px; border: 0; background: transparent; padding: 2px 0; text-align: right; cursor: pointer; font-weight: 600; color: var(--slate); }
  .sort.on { color: var(--teal-text); }
  .procs .list > .item { padding: 10px 4px; }
  .grow { flex: 1; min-width: 0; }
  .w { width: 90px; text-align: right; }
  .actions { width: 150px; display: flex; gap: 6px; justify-content: flex-end; }

  @media (max-width: 1000px) { .grid2 { grid-template-columns: 1fr; } }
</style>
