<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import JobProgress from "../components/JobProgress.svelte";
  import AdminBanner from "../components/AdminBanner.svelte";
  import { results, runJob, running } from "../lib/jobs.svelte";
  import { api } from "../lib/api";
  import { bytes, shortPath } from "../lib/format";
  import { toast, ui , t } from "../lib/state.svelte";
  import type { CleanerPreview } from "../lib/types";

  const CATS = $derived<Record<string, string>>({
    system: "Windows", browser: t("Browsers", "Browsers"), apps: t("Apps", "Apps"), games: t("Games", "Games"),
    developer: t("Developer tools", "Developer tools"), logs: t("Logs and crash dumps", "Logs aur crash dumps"),
    recycle_bin: "Recycle Bin", privacy: t("Privacy (history, cookies)", "Privacy (history, cookies)"),
  });

  // Results live in the shared store, so a scan keeps going (and stays) when you switch pages.
  const report = $derived(results.scan);
  const result = $derived(results.clean);
  const scanning = $derived(running("scan"));
  const cleaning = $derived(running("clean"));
  results.sel.clean ??= {};
  results.sel.cleanFiles ??= {};
  const selected = results.sel.clean;
  const approvedFiles = results.sel.cleanFiles;
  let open = $state<Record<string, boolean>>({});
  let confirm = $state(false);

  async function scan() {
    results.clean = null;
    const r = await runJob("scan", t("Scanning for junk", "Kachra dhoondh raha hoon"), "clean", () => api.scan(false, ui.settings?.privacy_consent ?? false));
    if (!r) return;
    for (const k of Object.keys(selected)) delete selected[k];
    for (const k of Object.keys(approvedFiles)) delete approvedFiles[k];
    for (const c of r.cleaners) selected[c.id] = c.status.kind === "ready" && c.risk === "safe";
    results.scan = r;
  }

  const groups = $derived.by(() => {
    if (!report) return [] as [string, CleanerPreview[]][];
    const m = new Map<string, CleanerPreview[]>();
    for (const c of report.cleaners) m.set(c.category, [...(m.get(c.category) ?? []), c]);
    return [...m.entries()];
  });
  const chosen = $derived(report ? report.cleaners.filter((c) => selected[c.id] && c.status.kind === "ready") : []);
  const flaggedChosen = $derived(chosen.flatMap((c) => c.flagged.filter((f) => approvedFiles[f.path])));
  const total = $derived(chosen.reduce((s, c) => s + c.bytes - c.flagged_bytes, 0) + flaggedChosen.reduce((s, f) => s + f.bytes, 0));
  const risky = $derived(chosen.filter((c) => c.risk !== "safe"));
  const needAdmin = $derived(report ? report.cleaners.filter((c) => c.status.kind === "needs_admin").length : 0);

  function readyOf(items: CleanerPreview[]): CleanerPreview[] {
    return items.filter((c) => c.status.kind === "ready");
  }
  function allReadyOn(items: CleanerPreview[]): boolean {
    const r = readyOf(items);
    return r.length > 0 && r.every((c) => selected[c.id]);
  }
  function toggleGroup(items: CleanerPreview[]) {
    const on = !allReadyOn(items);
    for (const c of readyOf(items)) selected[c.id] = on;
  }

  function start() {
    if (risky.length || flaggedChosen.length) confirm = true;
    else run(false);
  }

  async function run(confirmed: boolean) {
    confirm = false;
    const ids = chosen.map((c) => c.id), files = flaggedChosen.map((f) => f.path);
    const r = await runJob("clean", t("Cleaning", "Safai ho rahi hai"), "clean", () => api.clean(ids, confirmed, files));
    if (!r) return;
    results.clean = r;
    results.scan = null;
    toast(t(`Freed ${bytes(r.freed_bytes)}`, `${bytes(r.freed_bytes)} jagah khaali hui`));
  }

  function statusText(c: CleanerPreview): string {
    switch (c.status.kind) {
      case "empty": return t("Nothing found", "Kuch nahi mila");
      case "needs_admin": return t("Needs administrator", "Admin chahiye");
      case "app_running": return t(`Close first: ${c.status.apps.join(", ")}`, `Pehle band karein: ${c.status.apps.join(", ")}`);
      default: return "";
    }
  }
</script>

<header class="head">
  <div class="titles">
    <h1>{t("Clean", "Safai")}</h1>
    <p class="muted">{t("Temp files, caches, logs and leftovers. Look first, then choose.", "Temp files, cache, logs aur purana kachra. Pehle dekhiye, phir chuniye.")}</p>
  </div>
  <button class="btn primary big" onclick={scan} disabled={scanning || cleaning}>
    {#if scanning}<div class="spin" style="border-top-color:var(--on-primary)"></div>{t("Scanning…", "Scan ho raha hai…")}{:else}<Icon name="broom" />{report ? t("Scan again", "Dobara scan") : t("Scan", "Scan karein")}{/if}
  </button>
</header>

<JobProgress id="scan" hint={t("Looking at temp files, caches and logs…", "Temp files, cache aur logs dekh raha hoon…")} />
<JobProgress id="clean" />

{#if result}
  <div class="card done fade">
    <span class="big-ic"><Icon name="check" size={26} /></span>
    <div class="col" style="gap:4px">
      <h2>{t(`Freed ${bytes(result.freed_bytes)}`, `${bytes(result.freed_bytes)} jagah khaali hui`)}</h2>
      <p class="muted">{t(`Removed ${result.deleted_files.toLocaleString()} files.`, `${result.deleted_files.toLocaleString()} files hatayi.`)}
        {#if result.outcomes.some((o) => o.in_use)}{t("Some files were in use and were skipped.", "Kuch files kisi app ke use mein thi, unhe chhod diya.")}{/if}
        {#if result.outcomes.some((o) => o.recycled)}{t("Your files that you ticked went to the Recycle Bin.", "Aapki tick ki hui files Recycle Bin mein gayi hain.")}{/if}
        {#if result.outcomes.some((o) => o.kept_for_approval)}{t("Files that look like yours were not touched.", "Aapki files jaisi lagne wali files ko haath nahi lagaya.")}{/if}
        {t("Recorded in History.", "Yeh safai Hisaab mein likh di gayi hai.")}</p>
    </div>
  </div>
{/if}

{#if !report && !scanning && !result}
  <section class="card intro fade">
    <div class="intro-top">
      <span class="hero-ic"><Icon name="broom" size={26} /></span>
      <div class="col" style="gap:4px">
        <h2>{t("Free up space in a few clicks", "Kuch clicks mein jagah khaali karein")}</h2>
        <p class="muted">{t("Scan first to see what can go. Nothing is deleted until you choose and press Clean.", "Pehle scan karke dekhiye kya hat sakta hai. Aap chunkar Clean dabayenge tab tak kuch delete nahi hota.")}</p>
      </div>
    </div>
    <div class="grid3">
      <div class="tile">
        <span class="tile-ic"><Icon name="file" /></span>
        <strong>Windows</strong>
        <span class="muted small">{t("Temp files, thumbnail cache, crash dumps, update leftovers", "Temp files, thumbnail cache, crash dumps, update ka bacha data")}</span>
      </div>
      <div class="tile">
        <span class="tile-ic"><Icon name="sparkle" /></span>
        <strong>{t("Browsers and apps", "Browsers aur apps")}</strong>
        <span class="muted small">{t("Chrome, Edge, Firefox, Brave, Discord, Teams caches. Logins stay.", "Chrome, Edge, Firefox, Brave, Discord, Teams ka cache: login safe")}</span>
      </div>
      <div class="tile never">
        <span class="tile-ic"><Icon name="lock" /></span>
        <strong>{t("Never", "Kabhi nahi")}</strong>
        <span class="muted small">{t("Documents, Desktop, Pictures, Downloads: personal files are never touched", "Documents, Desktop, Photos, Downloads: personal files kabhi khud nahi")}</span>
      </div>
    </div>
  </section>
{/if}

{#if report}
  <div class="toolbar card flat">
    <div><strong class="num big">{bytes(total)}</strong> <span class="muted">{t(`selected · ${chosen.length} cleaners`, `chuna gaya · ${chosen.length} cleaners`)}</span></div>
    <span class="spacer"></span>
    <span class="muted small">{t(`Scanned in ${(report.duration_ms / 1000).toFixed(1)}s`, `Scan ${(report.duration_ms / 1000).toFixed(1)}s mein`)}</span>
    <button class="btn primary" onclick={start} disabled={!chosen.length || cleaning}>
      {#if cleaning}<div class="spin" style="border-top-color:var(--on-primary)"></div>{:else}<Icon name="broom" size={18} />{/if}{t("Clean now", "Saaf karein")}
    </button>
  </div>

  {#if needAdmin}
    <AdminBanner count={needAdmin} text={t("Some cleaners need administrator rights. After you restart Sweeplark as admin, they can run too. Everything else stays as it is.", "Kuch cleaners ko admin rights chahiye. Sweeplark ko admin mode mein kholne ke baad woh bhi chal jayenge. Baaki sab waise hi rahega.")} />
  {/if}

  {#each groups as [cat, items]}
    {@const readyItems = readyOf(items)}
    <section class="group">
      <div class="group-head">
        <h3 class="cat">{CATS[cat] ?? cat}</h3>
        {#if readyItems.length}
          <span class="muted small num">{bytes(readyItems.reduce((s, c) => s + c.bytes, 0))} {t("ready", "ready")}</span>
        {/if}
        <span class="spacer"></span>
        {#if readyItems.length}
          <button class="btn ghost small" onclick={() => toggleGroup(items)}>
            {allReadyOn(items) ? t("Select none", "Sab hatayein") : t("Select all", "Sab chunein")}
          </button>
        {/if}
      </div>
      <div class="card list">
        {#each items as c}
          <div class="item" class:dim={c.status.kind !== "ready"}>
            <input type="checkbox" bind:checked={selected[c.id]} disabled={c.status.kind !== "ready"} aria-label={c.name} />
            <button class="name" onclick={() => (open[c.id] = !open[c.id])} disabled={!c.files}>
              <span class="row wrap"><strong>{c.name}</strong>
                <span class="chip {c.risk}">{c.risk === "safe" ? "Safe" : c.risk === "ask" ? t("Asks first", "Poochega") : "Expert"}</span>
                {#if c.flagged.length}<span class="chip ask">{t(`${c.flagged.length} files may be yours`, `${c.flagged.length} files aapki ho sakti hain`)}</span>{/if}
              </span>
              <span class="muted small">{statusText(c) || c.description}</span>
            </button>
            <strong class="num size">{c.files ? bytes(c.bytes) : "–"}</strong>
            {#if c.files}<button class="btn ghost small" aria-label="Details" onclick={() => (open[c.id] = !open[c.id])}><Icon name={open[c.id] ? "down" : "chevron"} size={16} /></button>{/if}
          </div>
          {#if open[c.id]}
            <div class="details fade">
              {#if c.note}<p class="note"><Icon name="alert" size={15} />{c.note}</p>{/if}
              {#if c.flagged.length}
                <p class="small"><strong>{t("These are only removed if you tick them:", "In files ko tabhi hatayenge jab aap tick karenge:")}</strong></p>
                {#each c.flagged as f}
                  <label class="frow">
                    <input type="checkbox" bind:checked={approvedFiles[f.path]} />
                    <span class="chip">{f.kind}</span><span class="path selectable" title={f.path}>{shortPath(f.path, 70)}</span>
                    <span class="spacer"></span><span class="num muted">{bytes(f.bytes)}</span>
                  </label>
                  {#if f.review}<p class="review small">{f.review}</p>{/if}
                {/each}
              {/if}
              <p class="small muted">{t(`Largest files (of ${c.files.toLocaleString()}):`, `Sabse badi files (${c.files.toLocaleString()} mein se):`)}</p>
              {#each c.sample.slice(0, 8) as s}
                <div class="frow"><span class="path selectable" title={s.path}>{shortPath(s.path, 86)}</span><span class="spacer"></span><span class="num muted">{bytes(s.bytes)}</span></div>
              {/each}
              {#each c.blocked as b}<p class="small muted">{t("Blocked by safety:", "Safety ne roka:")} {b}</p>{/each}
            </div>
          {/if}
        {/each}
      </div>
    </section>
  {/each}
{/if}

<Modal bind:open={confirm} title={t("Clean these?", "Pakka saaf karein?")}>
  <div class="col">
    <p>{t("Please check these first:", "Yeh cheezein dhyan se dekh lein:")}</p>
    {#each risky as c}<div class="note"><Icon name="alert" size={15} /><span><strong>{c.name}</strong>{c.note ? `: ${c.note}` : ""}</span></div>{/each}
    {#if flaggedChosen.length}<div class="note"><Icon name="file" size={15} /><span>{t(`You picked ${flaggedChosen.length} files that may be yours. They go to the Recycle Bin, so you can restore them.`, `Aapne ${flaggedChosen.length} aisi files chuni hain jo aapki ho sakti hain. Ye Recycle Bin mein jayengi, wapas la sakte hain.`)}</span></div>{/if}
    <p class="muted small">{t("Deleted cache and temp files don't come back. Apps recreate them when needed.", "Cache aur temp files delete hone ke baad wapas nahi aatin. Apps unhe zarurat par dobara bana leti hain.")}</p>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => (confirm = false)}>{t("Cancel", "Ruko")}</button>
    <button class="btn primary" onclick={() => run(true)}>{t("Yes, clean", "Haan, saaf karo")}</button>
  {/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 20px; }
  .titles { display: flex; flex-direction: column; gap: 4px; }
  .toolbar { position: sticky; top: 0; z-index: 5; display: flex; align-items: center; gap: 14px; padding: 12px 16px; margin-bottom: 16px; background: color-mix(in srgb, var(--surface) 92%, transparent); backdrop-filter: blur(8px); }
  .big { font-size: 22px; }
  .group { margin-bottom: 16px; }
  .group-head { display: flex; align-items: center; gap: 10px; margin: 0 4px 8px; }
  .cat { font-size: 12.5px; text-transform: uppercase; letter-spacing: .06em; color: var(--slate); }
  .card.list { padding: 4px 14px; }
  .item.dim { opacity: .65; }
  .name { flex: 1; display: flex; flex-direction: column; align-items: flex-start; gap: 3px; background: none; border: 0; padding: 0; text-align: left; cursor: pointer; min-width: 0; }
  .name:disabled { cursor: default; }
  .size { min-width: 74px; text-align: right; }
  .details { padding: 4px 4px 14px 33px; display: flex; flex-direction: column; gap: 6px; border-top: 1px dashed var(--line); }
  .frow { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
  .path { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { display: flex; gap: 8px; align-items: flex-start; padding: 9px 11px; border-radius: 10px; background: var(--amber-soft); color: var(--amber-text); font-size: 13px; }
  .review { margin: -2px 0 4px 25px; color: var(--teal-text); }
  .done { display: flex; align-items: center; gap: 16px; margin-bottom: 16px; }
  .big-ic { display: grid; place-items: center; width: 48px; height: 48px; border-radius: 50%; background: var(--good); color: #fff; flex: none; }

  .intro { display: flex; flex-direction: column; gap: 18px; padding: 24px; }
  .intro-top { display: flex; align-items: center; gap: 16px; }
  .hero-ic { display: grid; place-items: center; width: 52px; height: 52px; border-radius: 16px; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .intro .grid3 { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 14px; }
  .tile { display: flex; flex-direction: column; gap: 6px; padding: 16px; border-radius: 14px; background: var(--surface-2); border: 1px solid var(--line); }
  .tile strong { color: var(--ink); font-size: 14px; }
  .tile-ic { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 10px; background: var(--surface); color: var(--teal-text); margin-bottom: 4px; }
  .tile.never .tile-ic { color: var(--amber-text); }
  @media (max-width: 760px) { .intro .grid3 { grid-template-columns: 1fr; } }
</style>
