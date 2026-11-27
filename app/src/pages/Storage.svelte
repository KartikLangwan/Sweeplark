<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import { api } from "../lib/api";
  import { bytes, shortPath } from "../lib/format";
  import { t, toast } from "../lib/state.svelte";
  import type { SizedPath, StorageReport } from "../lib/types";

  let report = $state<StorageReport | null>(null);
  let busy = $state(false);
  let path = $state("");
  let target = $state<SizedPath | null>(null);

  async function scan(p?: string) {
    busy = true;
    try { report = await api.storage(p || undefined); path = report.root; }
    catch (e) { toast(String(e), "warn"); }
    busy = false;
  }
  async function recycle() {
    if (!target) return;
    const p = target.path;
    target = null;
    try {
      await api.recycle(p);
      toast(t("Moved to the Recycle Bin", "Recycle Bin mein bhej diya"));
      if (report) report = { ...report, big_files: report.big_files.filter((f) => f.path !== p), folders: report.folders.filter((f) => f.path !== p) };
    } catch (e) { toast(String(e), "warn"); }
  }
  const max = $derived(report?.folders[0]?.bytes ?? 1);
  const COLORS = ["#0FB5AE", "#7DE8C8", "#FFB039", "#7c6cf2", "#5B6B80", "#e5484d", "#2fd3bc", "#a3b1c2"];
</script>

<header class="head">
  <div><h1>{t("Storage", "Storage")}</h1><p class="muted">{t("See what takes space: biggest folders, biggest files and file types. Removing anything sends it to the Recycle Bin.", "Dekhein jagah kaun le raha hai: sabse bade folders, files aur file types. Kuch bhi hatane par Recycle Bin mein jata hai.")}</p></div>
</header>

<div class="card bar-row">
  <input type="text" bind:value={path} placeholder={t("Folder or drive, e.g. C:\\ (empty = your user folder)", "Folder ya drive, jaise C:\\ (khaali = aapka user folder)")} style="flex:1" />
  <button class="btn primary" onclick={() => scan(path)} disabled={busy}>{#if busy}<div class="spin" style="border-top-color:#fff"></div>{t("Analyzing…", "Dekh raha hoon…")}{:else}<Icon name="disk" size={17} />{t("Analyze", "Analyze")}{/if}</button>
</div>

{#if report}
  <section class="summary">
    <div class="card stat"><strong class="num">{bytes(report.total_bytes)}</strong><span class="muted">{report.total_files.toLocaleString()} files{report.complete ? "" : t(" (partial)", " (adhoora)")}</span></div>
    <div class="card kinds">
      <div class="stack">{#each report.by_kind.slice(0, 8) as [k, b], i}<span style="width:{(b / Math.max(1, report.total_bytes)) * 100}%;background:{COLORS[i]}" title="{k}: {bytes(b)}"></span>{/each}</div>
      <div class="legend">{#each report.by_kind.slice(0, 8) as [k, b], i}<span><i style="background:{COLORS[i]}"></i>{k} <b class="num">{bytes(b)}</b></span>{/each}</div>
    </div>
  </section>
  <section class="two">
    <div class="card">
      <div class="card-title"><Icon name="file" /><h3>{t("Biggest folders", "Sabse bade folders")}</h3></div>
      <div class="list">
        {#each report.folders.slice(0, 15) as f}
          <div class="item"><div class="col grow" style="gap:4px">
            <button class="link selectable" onclick={() => scan(f.path)} title={f.path}>{shortPath(f.path, 60)}</button>
            <div class="bar"><span style="width:{(f.bytes / max) * 100}%"></span></div></div>
            <span class="num">{bytes(f.bytes)}</span>
            <button class="btn ghost small" onclick={() => api.reveal(f.path)} title={t("Show in Explorer", "Explorer mein dikhao")}><Icon name="chevron" size={14} /></button></div>
        {/each}
      </div>
    </div>
    <div class="card">
      <div class="card-title"><Icon name="download" /><h3>{t("Biggest files", "Sabse badi files")}</h3></div>
      <div class="list">
        {#each report.big_files.slice(0, 20) as f}
          <div class="item"><span class="path grow selectable" title={f.path}>{shortPath(f.path, 54)}</span><span class="num">{bytes(f.bytes)}</span>
            <button class="btn ghost small" onclick={() => api.reveal(f.path)}><Icon name="file" size={14} /></button>
            <button class="btn ghost small danger" onclick={() => (target = f)} title={t("Move to Recycle Bin", "Recycle Bin mein bhejo")}><Icon name="trash" size={14} /></button></div>
        {:else}<div class="empty">{t("No files over 50 MB.", "50 MB se badi koi file nahi.")}</div>{/each}
      </div>
    </div>
  </section>
{/if}

<Modal open={target !== null} title={t("Move to Recycle Bin?", "Recycle Bin mein bhejein?")}>
  {#if target}<p class="selectable path">{target.path}</p><p class="muted small" style="margin-top:8px">{t(`${bytes(target.bytes)}. You can restore it from the Recycle Bin.`, `${bytes(target.bytes)}. Recycle Bin se wapas la sakte hain.`)}</p>{/if}
  {#snippet footer()}<button class="btn" onclick={() => (target = null)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={recycle}>{t("Move", "Bhejo")}</button>{/snippet}
</Modal>

<style>
  .head { margin-bottom: 18px; }
  .bar-row { display: flex; gap: 10px; margin-bottom: 14px; padding: 12px; }
  .summary { display: grid; grid-template-columns: 200px 1fr; gap: 14px; margin-bottom: 14px; }
  .stat { display: flex; flex-direction: column; gap: 2px; }
  .stat strong { font-size: 24px; font-weight: 650; }
  .kinds { display: flex; flex-direction: column; gap: 10px; }
  .stack { display: flex; height: 12px; border-radius: 999px; overflow: hidden; background: var(--surface-3); }
  .legend { display: flex; flex-wrap: wrap; gap: 6px 16px; font-size: 12.5px; color: var(--slate); }
  .legend i { display: inline-block; width: 9px; height: 9px; border-radius: 3px; margin-right: 5px; }
  .legend b { color: var(--ink); font-weight: 600; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; align-items: start; }
  .grow { flex: 1; min-width: 0; }
  .link { background: none; border: 0; padding: 0; text-align: left; color: var(--ink); cursor: pointer; font-size: 13px; }
  .link:hover { color: var(--teal-text); text-decoration: underline; }
  .path { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  @media (max-width: 1000px) { .two, .summary { grid-template-columns: 1fr; } }
</style>
