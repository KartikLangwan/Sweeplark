<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import JobProgress from "../components/JobProgress.svelte";
  import { api } from "../lib/api";
  import { bytes } from "../lib/format";
  import { results, runJob, running } from "../lib/jobs.svelte";
  import { t, toast } from "../lib/state.svelte";
  import type { InstalledApp } from "../lib/types";

  let tab = $state<"installed" | "updates">("installed");
  let query = $state("");
  let sort = $state<"name" | "size">("size");
  let target = $state<InstalledApp | null>(null);
  let waiting = $state<InstalledApp | null>(null);
  let leftovers = $state<string[] | null>(null);
  let pickedLeft = $state<Record<string, boolean>>({});

  const apps = $derived(results.apps);
  const updates = $derived(results.updates);

  onMount(() => { if (!results.apps) load(); });

  async function load() {
    const r = await runJob("apps", t("Reading installed apps", "Installed apps padh raha hoon"), "apps", () => api.apps());
    if (r) results.apps = r;
  }
  async function checkUpdates() {
    const r = await runJob("updates", t("Checking app updates", "App updates dekh raha hoon"), "apps", () => api.appUpdates());
    if (r) results.updates = r;
  }

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = (apps ?? []).filter((a) => !q || (a.name + " " + (a.publisher ?? "")).toLowerCase().includes(q));
    return sort === "size" ? [...list].sort((a, b) => (b.size_bytes ?? 0) - (a.size_bytes ?? 0)) : list;
  });
  const totalSize = $derived((apps ?? []).reduce((s, a) => s + (a.size_bytes ?? 0), 0));

  async function uninstall() {
    const a = target;
    target = null;
    if (!a) return;
    try {
      await api.uninstall(a.id);
      waiting = a;
      leftovers = null;
      toast(t(`${a.name}'s uninstaller opened. Finish it, then check for leftovers.`, `${a.name} ka uninstaller khula. Use poora karein, phir bacha hua kachra dekhein.`), "info");
    } catch (e) { toast(String(e), "warn"); }
  }
  async function findLeftovers() {
    try {
      leftovers = await api.leftovers();
      pickedLeft = Object.fromEntries(leftovers.map((p) => [p, true]));
      await load();
    } catch (e) { toast(String(e), "warn"); }
  }
  async function removeLeftovers() {
    let ok = 0;
    for (const p of (leftovers ?? []).filter((p) => pickedLeft[p])) {
      try { await api.recycle(p); ok++; } catch (e) { toast(String(e), "warn"); }
    }
    toast(t(`${ok} leftover folders moved to the Recycle Bin`, `${ok} bache folders Recycle Bin mein bheje`));
    leftovers = null;
    waiting = null;
  }
  async function update(id: string | null) {
    try { await api.appUpdate(id); toast(t("Update started in a new window", "Update nayi window mein shuru hua"), "info"); }
    catch (e) { toast(String(e), "warn"); }
  }
  const date = (d: string | null) => (d && d.length === 8 ? `${d.slice(6, 8)}.${d.slice(4, 6)}.${d.slice(0, 4)}` : "");
</script>

<header class="head">
  <div><h1>{t("Apps", "Apps")}</h1><p class="muted">{t("Remove programs you don't use (with their leftovers) and keep the rest up to date.", "Bekaar programs hatayein (bache kachre ke saath) aur baaki ko update rakhein.")}</p></div>
  <div class="seg"><button class:on={tab === "installed"} onclick={() => (tab = "installed")}>{t("Installed", "Installed")}</button>
    <button class:on={tab === "updates"} onclick={() => { tab = "updates"; if (!results.updates) checkUpdates(); }}>{t("Updates", "Updates")}{#if updates?.length}<em>{updates.length}</em>{/if}</button></div>
</header>

{#if tab === "installed"}
  {#if waiting}
    <div class="card wait fade">
      <Icon name="info" />
      <div class="col grow" style="gap:4px"><strong>{t(`Finished uninstalling ${waiting.name}?`, `${waiting.name} ka uninstall poora hua?`)}</strong>
        <span class="muted small">{t("Sweeplark can look for folders it left behind in AppData and Program Files.", "Sweeplark AppData aur Program Files mein bache folders dhoondh sakta hai.")}</span></div>
      <button class="btn" onclick={() => (waiting = null)}>{t("Later", "Baad mein")}</button>
      <button class="btn primary" onclick={findLeftovers}>{t("Find leftovers", "Bacha kachra dhoondho")}</button>
    </div>
  {/if}
  <JobProgress id="apps" />
  <div class="card bar-row">
    <input type="text" bind:value={query} placeholder={t("Search apps…", "Apps dhoondein…")} style="flex:1" aria-label={t("Search apps", "Apps dhoondein")} />
    <select bind:value={sort} aria-label={t("Sort", "Sort")}><option value="size">{t("Biggest first", "Sabse bade pehle")}</option><option value="name">{t("By name", "Naam se")}</option></select>
    <button class="btn ghost small" onclick={load} disabled={running("apps")}><Icon name="restart" size={14} />{t("Refresh", "Refresh")}</button>
  </div>
  {#if apps}
    <p class="muted small" style="margin:0 4px 8px">{t(`${apps.length} apps · about ${bytes(totalSize)}`, `${apps.length} apps · lagbhag ${bytes(totalSize)}`)}</p>
    <div class="card list">
      {#each shown as a (a.id)}
        <div class="item">
          <span class="ic">{a.name.slice(0, 1).toUpperCase()}</span>
          <div class="col grow" style="gap:1px"><strong>{a.name}</strong>
            <span class="muted small">{[a.publisher, a.version, date(a.install_date), a.for_all_users ? t("all users", "sab users") : ""].filter(Boolean).join(" · ")}</span></div>
          <span class="num muted">{a.size_bytes ? bytes(a.size_bytes) : ""}</span>
          <button class="btn small danger" onclick={() => (target = a)}>{t("Uninstall", "Hatayein")}</button>
        </div>
      {:else}<div class="empty">{t("No apps match.", "Koi app nahi mili.")}</div>{/each}
    </div>
  {/if}
{:else}
  <JobProgress id="updates" hint={t("Asking winget which apps have newer versions…", "winget se naye versions pooch raha hoon…")} />
  <div class="card bar-row">
    <span class="muted small grow">{t("Updates come from winget (Windows Package Manager). Each one opens in its own window so you can see it.", "Updates winget (Windows Package Manager) se aate hain. Har update apni window mein dikhta hai.")}</span>
    <button class="btn ghost small" onclick={checkUpdates} disabled={running("updates")}><Icon name="restart" size={14} />{t("Check again", "Dobara dekho")}</button>
    {#if updates?.length}<button class="btn primary small" onclick={() => update(null)}><Icon name="download" size={14} />{t("Update all", "Sab update karo")}</button>{/if}
  </div>
  {#if updates}
    <div class="card list">
      {#each updates as u (u.id)}
        <div class="item"><div class="col grow" style="gap:1px"><strong>{u.name}</strong><span class="muted small">{u.id}</span></div>
          <span class="num muted small">{u.current} → <b class="new">{u.available}</b></span>
          <button class="btn small" onclick={() => update(u.id)}>{t("Update", "Update")}</button></div>
      {:else}<div class="empty"><Icon name="check" />{t("Everything is up to date.", "Sab kuch up to date hai.")}</div>{/each}
    </div>
  {/if}
{/if}

<Modal open={target !== null} title={t("Uninstall this app?", "Yeh app hatayein?")}>
  {#if target}<p><strong>{target.name}</strong></p>
    <p class="muted small" style="margin-top:6px">{t("Its own uninstaller opens; follow its steps. Afterwards Sweeplark can look for leftover folders.", "Iska apna uninstaller khulega; uske steps follow karein. Baad mein Sweeplark bache folders dhoondh sakta hai.")}</p>{/if}
  {#snippet footer()}<button class="btn" onclick={() => (target = null)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={uninstall}>{t("Open uninstaller", "Uninstaller kholo")}</button>{/snippet}
</Modal>

<Modal open={leftovers !== null} title={t("Leftover folders", "Bache hue folders")} wide>
  {#if leftovers?.length}
    <p class="muted small" style="margin-bottom:8px">{t("Only folders named like the app. Untick anything you want to keep. They go to the Recycle Bin.", "Sirf app ke naam wale folders. Jo rakhna ho use untick karein. Ye Recycle Bin mein jayenge.")}</p>
    {#each leftovers as p}<label class="frow"><input type="checkbox" bind:checked={pickedLeft[p]} /><span class="path selectable">{p}</span></label>{/each}
  {:else}<p>{t("Nothing left behind. Clean uninstall!", "Kuch nahi bacha. Saaf uninstall!")}</p>{/if}
  {#snippet footer()}<button class="btn" onclick={() => { leftovers = null; waiting = null; }}>{t("Close", "Band")}</button>
    {#if leftovers?.length}<button class="btn primary" onclick={removeLeftovers}>{t("Move to Recycle Bin", "Recycle Bin mein bhejo")}</button>{/if}{/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 18px; flex-wrap: wrap; }
  .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: var(--surface-3); }
  .seg button { border: 0; background: transparent; padding: 6px 16px; border-radius: 999px; cursor: pointer; font-weight: 560; color: var(--slate); display: inline-flex; gap: 6px; align-items: center; }
  .seg button.on { background: var(--surface); color: var(--teal-text); box-shadow: var(--shadow); }
  .seg em { font-style: normal; font-size: 11px; background: var(--amber); color: #3a2500; border-radius: 999px; padding: 0 6px; }
  .bar-row { display: flex; gap: 10px; margin-bottom: 12px; padding: 12px; align-items: center; flex-wrap: wrap; }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .ic { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 9px; background: var(--teal-soft); color: var(--teal-text); font-weight: 700; flex: none; }
  .wait { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; border-color: var(--teal); flex-wrap: wrap; }
  .new { color: var(--teal-text); }
  .frow { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
  .path { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; word-break: break-all; }
</style>
