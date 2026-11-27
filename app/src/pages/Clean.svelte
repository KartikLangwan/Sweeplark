<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import { api } from "../lib/api";
  import { bytes, shortPath } from "../lib/format";
  import { toast, ui } from "../lib/state.svelte";
  import type { CleanReport, CleanerPreview, ScanReport } from "../lib/types";

  const CATS: Record<string, string> = {
    system: "Windows", browser: "Browsers", apps: "Apps", games: "Games", developer: "Developer tools",
    logs: "Logs aur crash dumps", recycle_bin: "Recycle Bin", privacy: "Privacy (history, cookies)",
  };

  let report = $state<ScanReport | null>(null);
  let scanning = $state(false);
  let cleaning = $state(false);
  let selected = $state<Record<string, boolean>>({});
  let approvedFiles = $state<Record<string, boolean>>({});
  let open = $state<Record<string, boolean>>({});
  let confirm = $state(false);
  let result = $state<CleanReport | null>(null);

  async function scan() {
    scanning = true;
    result = null;
    try {
      report = await api.scan(false, ui.settings?.privacy_consent ?? false);
      selected = Object.fromEntries(report.cleaners.map((c) => [c.id, c.status.kind === "ready" && c.risk === "safe"]));
      approvedFiles = {};
    } catch (e) {
      toast(String(e), "warn");
    } finally {
      scanning = false;
    }
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

  function start() {
    if (risky.length || flaggedChosen.length) confirm = true;
    else run(false);
  }

  async function run(confirmed: boolean) {
    confirm = false;
    cleaning = true;
    try {
      result = await api.clean(chosen.map((c) => c.id), confirmed, flaggedChosen.map((f) => f.path));
      toast(`${bytes(result.freed_bytes)} jagah khaali hui`);
      report = null;
    } catch (e) {
      toast(String(e), "warn");
    } finally {
      cleaning = false;
    }
  }

  function statusText(c: CleanerPreview): string {
    switch (c.status.kind) {
      case "empty": return "Kuch nahi mila";
      case "needs_admin": return "Admin chahiye";
      case "app_running": return `Pehle band karein: ${c.status.apps.join(", ")}`;
      default: return "";
    }
  }
</script>

<header class="head">
  <div><h1>Safai</h1><p class="muted">Temp files, cache, logs aur purana kachra. Pehle dekhiye, phir chuniye.</p></div>
  <button class="btn primary big" onclick={scan} disabled={scanning || cleaning}>
    {#if scanning}<div class="spin" style="border-top-color:#fff"></div>Scan ho raha hai…{:else}<Icon name="broom" />{report ? "Dobara scan" : "Scan karein"}{/if}
  </button>
</header>

{#if result}
  <div class="card done fade">
    <span class="big-ic"><Icon name="check" size={26} /></span>
    <div class="col" style="gap:4px">
      <h2>{bytes(result.freed_bytes)} jagah khaali hui</h2>
      <p class="muted">{result.deleted_files.toLocaleString()} files hatayi.
        {#if result.outcomes.some((o) => o.in_use)}Kuch files kisi app ke use mein thi, unhe chhod diya.{/if}
        {#if result.outcomes.some((o) => o.kept_for_approval)}Aapki files jaisi lagne wali files ko haath nahi lagaya.{/if}
        Yeh safai "Hisaab" mein likh di gayi hai.</p>
    </div>
  </div>
{/if}

{#if !report && !scanning && !result}
  <div class="card intro">
    <div class="col">
      <h2>Kya-kya saaf hota hai</h2>
      <div class="grid3">
        <div><Icon name="file" /><strong>Windows</strong><span class="muted small">Temp files, thumbnail cache, crash dumps, update ka bacha data</span></div>
        <div><Icon name="sparkle" /><strong>Browsers aur apps</strong><span class="muted small">Chrome, Edge, Firefox, Brave, Discord, Teams ka cache: login safe</span></div>
        <div><Icon name="lock" /><strong>Kabhi nahi</strong><span class="muted small">Documents, Desktop, Photos, Downloads: personal files kabhi khud nahi</span></div>
      </div>
    </div>
  </div>
{/if}

{#if report}
  <div class="toolbar card flat">
    <div><strong class="num big">{bytes(total)}</strong> <span class="muted">chuna gaya · {chosen.length} cleaners</span></div>
    <span class="spacer"></span>
    <span class="muted small">Scan {(report.duration_ms / 1000).toFixed(1)}s mein</span>
    <button class="btn primary" onclick={start} disabled={!chosen.length || cleaning}>
      {#if cleaning}<div class="spin" style="border-top-color:#fff"></div>{:else}<Icon name="broom" size={18} />{/if}Saaf karein
    </button>
  </div>

  {#each groups as [cat, items]}
    <section class="group">
      <h3 class="cat">{CATS[cat] ?? cat}</h3>
      <div class="card list">
        {#each items as c}
          <div class="item" class:dim={c.status.kind !== "ready"}>
            <input type="checkbox" bind:checked={selected[c.id]} disabled={c.status.kind !== "ready"} aria-label={c.name} />
            <button class="name" onclick={() => (open[c.id] = !open[c.id])} disabled={!c.files}>
              <span class="row"><strong>{c.name}</strong>
                <span class="chip {c.risk}">{c.risk === "safe" ? "Safe" : c.risk === "ask" ? "Poochega" : "Expert"}</span>
                {#if c.flagged.length}<span class="chip ask">{c.flagged.length} files aapki ho sakti hain</span>{/if}
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
                <p class="small"><strong>In files ko tabhi hatayenge jab aap tick karenge:</strong></p>
                {#each c.flagged as f}
                  <label class="frow">
                    <input type="checkbox" bind:checked={approvedFiles[f.path]} />
                    <span class="chip">{f.kind}</span><span class="path selectable" title={f.path}>{shortPath(f.path, 70)}</span>
                    <span class="spacer"></span><span class="num muted">{bytes(f.bytes)}</span>
                  </label>
                  {#if f.review}<p class="review small">{f.review}</p>{/if}
                {/each}
              {/if}
              <p class="small muted">Sabse badi files ({c.files.toLocaleString()} mein se):</p>
              {#each c.sample.slice(0, 8) as s}
                <div class="frow"><span class="path selectable" title={s.path}>{shortPath(s.path, 86)}</span><span class="spacer"></span><span class="num muted">{bytes(s.bytes)}</span></div>
              {/each}
              {#each c.blocked as b}<p class="small muted">Safety ne roka: {b}</p>{/each}
            </div>
          {/if}
        {/each}
      </div>
    </section>
  {/each}
{/if}

<Modal bind:open={confirm} title="Pakka saaf karein?">
  <div class="col">
    <p>Yeh cheezein dhyan se dekh lein:</p>
    {#each risky as c}<div class="note"><Icon name="alert" size={15} /><span><strong>{c.name}</strong>{c.note ? `: ${c.note}` : ""}</span></div>{/each}
    {#if flaggedChosen.length}<div class="note"><Icon name="file" size={15} /><span>Aapne {flaggedChosen.length} aisi files chuni hain jo aapki ho sakti hain. Ye wapas nahi aayengi.</span></div>{/if}
    <p class="muted small">Cache aur temp files delete hone ke baad wapas nahi aatin. Apps unhe zarurat par dobara bana leti hain.</p>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => (confirm = false)}>Ruko</button>
    <button class="btn primary" onclick={() => run(true)}>Haan, saaf karo</button>
  {/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .toolbar { position: sticky; top: 0; z-index: 5; display: flex; align-items: center; gap: 14px; padding: 12px 16px; margin-bottom: 14px; background: color-mix(in srgb, var(--surface) 92%, transparent); backdrop-filter: blur(8px); }
  .big { font-size: 22px; }
  .group { margin-bottom: 14px; }
  .cat { font-size: 12.5px; text-transform: uppercase; letter-spacing: .06em; color: var(--slate); margin: 0 0 8px 4px; }
  .card.list { padding: 4px 14px; }
  .item.dim { opacity: .65; }
  .name { flex: 1; display: flex; flex-direction: column; align-items: flex-start; gap: 2px; background: none; border: 0; padding: 0; text-align: left; cursor: pointer; min-width: 0; }
  .name:disabled { cursor: default; }
  .size { min-width: 74px; text-align: right; }
  .details { padding: 4px 4px 14px 33px; display: flex; flex-direction: column; gap: 6px; border-top: 1px dashed var(--line); }
  .frow { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
  .path { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { display: flex; gap: 8px; align-items: flex-start; padding: 9px 11px; border-radius: 10px; background: var(--amber-soft); color: var(--amber-text); font-size: 13px; }
  .review { margin: -2px 0 4px 25px; color: var(--teal-text); }
  .done { display: flex; align-items: center; gap: 16px; margin-bottom: 16px; background: linear-gradient(120deg, var(--surface), var(--good-soft)); }
  .big-ic { display: grid; place-items: center; width: 48px; height: 48px; border-radius: 50%; background: var(--good); color: #fff; flex: none; }
  .intro .grid3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; margin-top: 6px; }
  .intro .grid3 > div { display: flex; flex-direction: column; gap: 6px; padding: 14px; border-radius: 12px; background: var(--surface-2); border: 1px solid var(--line); color: var(--teal-text); }
  .intro .grid3 strong { color: var(--ink); }
</style>
