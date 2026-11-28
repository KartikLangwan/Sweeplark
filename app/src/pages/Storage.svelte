<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import JobProgress from "../components/JobProgress.svelte";
  import { api } from "../lib/api";
  import { bytes, shortPath } from "../lib/format";
  import { results, runJob, running } from "../lib/jobs.svelte";
  import { t, toast } from "../lib/state.svelte";
  import type { SizedPath } from "../lib/types";

  let tab = $state<"space" | "dupes">("space");
  let path = $state(results.storage?.root ?? "");
  let minMb = $state(1);
  let target = $state<SizedPath | null>(null);
  let confirmDupes = $state(false);
  results.sel.dupes ??= {};
  const picked = results.sel.dupes;

  const report = $derived(results.storage);
  const dupes = $derived(results.duplicates);
  const busy = $derived(running("storage"));
  const dupBusy = $derived(running("duplicates"));

  async function scan(p?: string) {
    const r = await runJob("storage", t("Analyzing storage", "Storage dekh raha hoon"), "storage", () => api.storage(p || undefined));
    if (r) { results.storage = r; path = r.root; }
  }

  async function findDupes() {
    const r = await runJob("duplicates", t("Finding duplicates", "Duplicate files dhoondh raha hoon"), "storage", () => api.duplicates(path || undefined, minMb));
    if (!r) return;
    for (const k of Object.keys(picked)) delete picked[k];
    // Suggest removing every copy except the oldest (the first) one.
    for (const g of r.groups) g.files.slice(1).forEach((f) => (picked[f.path] = true));
    results.duplicates = r;
  }

  async function recycle() {
    if (!target) return;
    const p = target.path;
    target = null;
    try {
      await api.recycle(p);
      toast(t("Moved to the Recycle Bin", "Recycle Bin mein bhej diya"));
      if (results.storage) results.storage = { ...results.storage, big_files: results.storage.big_files.filter((f) => f.path !== p), folders: results.storage.folders.filter((f) => f.path !== p) };
    } catch (e) { toast(String(e), "warn"); }
  }

  const chosen = $derived(dupes ? dupes.groups.flatMap((g) => g.files.filter((f) => picked[f.path])) : []);
  const chosenBytes = $derived(chosen.reduce((s, f) => s + f.bytes, 0));
  // Never all copies of one file: at least one stays.
  const unsafeGroup = $derived(dupes ? dupes.groups.some((g) => g.files.every((f) => picked[f.path])) : false);

  async function removeDupes() {
    confirmDupes = false;
    let ok = 0;
    for (const f of chosen) {
      try { await api.recycle(f.path); ok++; delete picked[f.path]; } catch (e) { toast(String(e), "warn"); }
    }
    toast(t(`${ok} copies moved to the Recycle Bin`, `${ok} copies Recycle Bin mein bheji`));
    if (results.duplicates) {
      const gone = new Set(chosen.map((f) => f.path));
      results.duplicates = { ...results.duplicates, groups: results.duplicates.groups.map((g) => ({ ...g, files: g.files.filter((f) => !gone.has(f.path)) })).filter((g) => g.files.length > 1) };
    }
  }

  const age = (s?: number) => {
    if (!s) return "";
    const days = Math.floor((Date.now() / 1000 - s) / 86400);
    return days < 1 ? t("today", "aaj") : days < 60 ? t(`${days} days old`, `${days} din purani`) : t(`${Math.floor(days / 30)} months old`, `${Math.floor(days / 30)} mahine purani`);
  };
  const max = $derived(report?.folders[0]?.bytes ?? 1);
  const COLORS = ["#0FB5AE", "#7DE8C8", "#FFB039", "#7c6cf2", "#5B6B80", "#e5484d", "#2fd3bc", "#a3b1c2"];
</script>

<header class="head">
  <div><h1>{t("Storage", "Storage")}</h1><p class="muted">{t("See what takes space and find duplicate files. Removing anything sends it to the Recycle Bin.", "Dekhein jagah kaun le raha hai aur duplicate files dhoondhein. Kuch bhi hatane par Recycle Bin mein jata hai.")}</p></div>
  <div class="seg"><button class:on={tab === "space"} onclick={() => (tab = "space")}>{t("What uses space", "Jagah kaun le raha")}</button>
    <button class:on={tab === "dupes"} onclick={() => (tab = "dupes")}>{t("Duplicates", "Duplicates")}{#if dupes?.groups.length}<em>{dupes.groups.length}</em>{/if}</button></div>
</header>

<div class="card bar-row">
  <input type="text" bind:value={path} placeholder={t("Folder or drive, e.g. C:\\ (empty = your user folder)", "Folder ya drive, jaise C:\\ (khaali = aapka user folder)")} style="flex:1" aria-label={t("Folder", "Folder")} />
  {#if tab === "dupes"}
    <label class="row small muted">{t("Min size", "Kam se kam")}<select bind:value={minMb}><option value={1}>1 MB</option><option value={10}>10 MB</option><option value={100}>100 MB</option></select></label>
    <button class="btn primary" onclick={findDupes} disabled={dupBusy}><Icon name="copy" size={17} />{t("Find duplicates", "Duplicates dhoondho")}</button>
  {:else}
    <button class="btn primary" onclick={() => scan(path)} disabled={busy}><Icon name="disk" size={17} />{t("Analyze", "Analyze")}</button>
  {/if}
</div>

{#if tab === "space"}
  <JobProgress id="storage" hint={t("Adding up folder sizes…", "Folders ka size jod raha hoon…")} />
  {#if report}
    <section class="summary">
      <div class="card stat"><strong class="num">{bytes(report.total_bytes)}</strong><span class="muted">{report.total_files.toLocaleString()} files{report.complete ? "" : t(" (stopped early: very large folder)", " (beech mein roka: bahut bada folder)")}</span></div>
      <div class="card kinds">
        <div class="stack">{#each report.by_kind.slice(0, 8) as [k, b], i}<span style="width:{(b / Math.max(1, report.total_bytes)) * 100}%;background:{COLORS[i]}" title="{k}: {bytes(b)}"></span>{/each}</div>
        <div class="legend">{#each report.by_kind.slice(0, 8) as [k, b], i}<span><i style="background:{COLORS[i]}"></i>{k} <b class="num">{bytes(b)}</b></span>{/each}</div>
      </div>
    </section>
    <section class="two">
      <div class="card">
        <div class="card-title"><Icon name="file" /><h3>{t("Biggest folders", "Sabse bade folders")}</h3></div>
        <p class="muted small" style="margin-bottom:6px">{t("Click a folder to look inside it.", "Andar dekhne ke liye folder par click karein.")}</p>
        <div class="list">
          {#each report.folders.slice(0, 15) as f}
            <div class="item"><div class="col grow" style="gap:4px">
              <button class="link selectable" onclick={() => scan(f.path)} title={f.path}>{shortPath(f.path, 60)}</button>
              <div class="bar"><span style="width:{(f.bytes / max) * 100}%"></span></div></div>
              <span class="num">{bytes(f.bytes)}</span>
              <button class="btn ghost small" onclick={() => api.reveal(f.path)} title={t("Show in Explorer", "Explorer mein dikhao")} aria-label={t("Show in Explorer", "Explorer mein dikhao")}><Icon name="chevron" size={14} /></button></div>
          {/each}
        </div>
      </div>
      <div class="card">
        <div class="card-title"><Icon name="download" /><h3>{t("Biggest files", "Sabse badi files")}</h3></div>
        <div class="list">
          {#each report.big_files.slice(0, 25) as f}
            <div class="item"><div class="col grow" style="gap:1px"><span class="path selectable" title={f.path}>{shortPath(f.path, 54)}</span><span class="muted small">{age(f.modified)}</span></div><span class="num">{bytes(f.bytes)}</span>
              <button class="btn ghost small" onclick={() => api.reveal(f.path)} aria-label={t("Show in Explorer", "Explorer mein dikhao")}><Icon name="file" size={14} /></button>
              <button class="btn ghost small danger" onclick={() => (target = f)} title={t("Move to Recycle Bin", "Recycle Bin mein bhejo")} aria-label={t("Move to Recycle Bin", "Recycle Bin mein bhejo")}><Icon name="trash" size={14} /></button></div>
          {:else}<div class="empty">{t("No files over 50 MB.", "50 MB se badi koi file nahi.")}</div>{/each}
        </div>
      </div>
    </section>
  {:else if !busy}
    <div class="card empty-state"><Icon name="disk" size={28} /><p>{t("Press Analyze to see which folders and files take the most space.", "Analyze dabaiye: kaunse folders aur files sabse zyada jagah le rahe hain.")}</p></div>
  {/if}
{:else}
  <JobProgress id="duplicates" hint={t("Comparing files byte by byte…", "Files ko byte-byte mila raha hoon…")} />
  {#if dupes}
    <div class="card toolbar">
      <div><strong class="num">{bytes(dupes.wasted_bytes)}</strong> <span class="muted">{t(`in extra copies · ${dupes.groups.length} files with copies`, `extra copies mein · ${dupes.groups.length} files ki copies`)}</span></div>
      <span class="spacer"></span>
      <span class="muted small">{t(`${chosen.length} picked · ${bytes(chosenBytes)}`, `${chosen.length} chuni · ${bytes(chosenBytes)}`)}</span>
      <button class="btn primary" disabled={!chosen.length || unsafeGroup} onclick={() => (confirmDupes = true)}><Icon name="trash" size={16} />{t("Move picked to Recycle Bin", "Chuni hui Recycle Bin mein")}</button>
    </div>
    {#if unsafeGroup}<p class="note small"><Icon name="alert" size={14} />{t("Keep at least one copy of every file.", "Har file ki kam se kam ek copy rakhein.")}</p>{/if}
    <p class="muted small" style="margin:6px 4px 10px">{t("The oldest copy of each file is kept by default. Contents are compared exactly, not just names.", "Har file ki sabse purani copy default mein rakhi jaati hai. Naam nahi, andar ka data poora milaya jata hai.")}</p>
    {#each dupes.groups.slice(0, 100) as g}
      <div class="card group">
        <div class="row small"><strong class="num">{bytes(g.bytes)}</strong><span class="muted">× {g.files.length}</span></div>
        {#each g.files as f, i}
          <label class="frow"><input type="checkbox" bind:checked={picked[f.path]} />
            <span class="path selectable grow" title={f.path}>{shortPath(f.path, 80)}</span>
            {#if i === 0}<span class="chip teal">{t("oldest", "sabse purani")}</span>{/if}
            <span class="muted small">{age(f.modified)}</span>
            <button class="btn ghost small" onclick={(e) => { e.preventDefault(); api.reveal(f.path); }} aria-label={t("Show in Explorer", "Explorer mein dikhao")}><Icon name="file" size={13} /></button></label>
        {/each}
      </div>
    {:else}<div class="card empty-state"><Icon name="check" size={28} /><p>{t("No duplicate files found here.", "Yahan koi duplicate file nahi mili.")}</p></div>{/each}
  {:else if !dupBusy}
    <div class="card empty-state"><Icon name="copy" size={28} /><p>{t("Find files you have more than once (photos, videos, downloads). Pick a folder, or leave it empty for your user folder.", "Ek hi file ki kai copies dhoondhein (photos, videos, downloads). Folder chunein, ya khaali chhodein.")}</p></div>
  {/if}
{/if}

<Modal open={target !== null} onclose={() => (target = null)} title={t("Move to Recycle Bin?", "Recycle Bin mein bhejein?")}>
  {#if target}<p class="selectable path">{target.path}</p><p class="muted small" style="margin-top:8px">{t(`${bytes(target.bytes)}. You can restore it from the Recycle Bin.`, `${bytes(target.bytes)}. Recycle Bin se wapas la sakte hain.`)}</p>{/if}
  {#snippet footer()}<button class="btn" onclick={() => (target = null)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={recycle}>{t("Move", "Bhejo")}</button>{/snippet}
</Modal>

<Modal bind:open={confirmDupes} title={t("Move these copies to the Recycle Bin?", "Ye copies Recycle Bin mein bhejein?")}>
  <p>{t(`${chosen.length} files, ${bytes(chosenBytes)}. One copy of each file stays where it is. You can restore them from the Recycle Bin.`, `${chosen.length} files, ${bytes(chosenBytes)}. Har file ki ek copy apni jagah rahegi. Recycle Bin se wapas la sakte hain.`)}</p>
  {#snippet footer()}<button class="btn" onclick={() => (confirmDupes = false)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={removeDupes}>{t("Move", "Bhejo")}</button>{/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 18px; flex-wrap: wrap; }
  .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: var(--surface-3); }
  .seg button { border: 0; background: transparent; padding: 6px 16px; border-radius: 999px; cursor: pointer; font-weight: 560; color: var(--slate); display: inline-flex; gap: 6px; align-items: center; }
  .seg button.on { background: var(--surface); color: var(--teal-text); box-shadow: var(--shadow); }
  .seg em { font-style: normal; font-size: 11px; background: var(--primary-bg); color: var(--on-primary); border-radius: 999px; padding: 0 6px; }
  .bar-row { display: flex; gap: 10px; margin-bottom: 14px; padding: 12px; align-items: center; flex-wrap: wrap; }
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
  .toolbar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; position: sticky; top: 0; z-index: 4; }
  .group { padding: 10px 14px; margin-bottom: 8px; display: flex; flex-direction: column; gap: 4px; }
  .frow { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
  .note { display: flex; gap: 6px; align-items: center; color: var(--amber-text); margin: 8px 4px; }
  .empty-state { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 36px; text-align: center; color: var(--slate); }
  @media (max-width: 1000px) { .two, .summary { grid-template-columns: 1fr; } }
</style>
