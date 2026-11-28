<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Modal from "../components/Modal.svelte";
  import JobProgress from "../components/JobProgress.svelte";
  import AdminBanner from "../components/AdminBanner.svelte";
  import { runJob, running } from "../lib/jobs.svelte";
  import { api } from "../lib/api";
  import { t, toast, ui } from "../lib/state.svelte";
  import type { TweakCategory, TweakState } from "../lib/types";

  type Tab = TweakCategory | "recommended";

  let tweaks = $state<TweakState[] | null>(null);
  let tab = $state<Tab>("recommended");
  let busy = $state<string | null>(null);
  let query = $state("");
  let confirm = $state(false);
  const applying = $derived(running("tweaks"));

  const TABS = $derived<[Tab, string, string][]>([
    ["recommended", t("Recommended", "Recommended"), "sparkle"],
    ["performance", t("Performance", "Performance"), "rocket"],
    ["gaming", t("Gaming", "Gaming"), "pulse"],
    ["gpu", "GPU", "cpu"],
    ["power", t("Power", "Power"), "battery"],
    ["privacy", t("Privacy", "Privacy"), "lock"],
    ["ai", t("Windows AI", "Windows AI"), "bird"],
    ["services", t("Services", "Services"), "settings"],
    ["explorer", t("Explorer", "Explorer"), "file"],
    ["taskbar", t("Taskbar", "Taskbar"), "home"],
    ["system", t("System", "System"), "shield"],
  ]);

  // One short line under the tabs that says what the active category is about.
  const TAB_HINT = $derived<Record<Tab, string>>({
    recommended: t("The safe picks most people want. One click applies them all.", "Zyaadatar logon ke liye safe chunein. Ek click mein sab lag jayenge."),
    performance: t("Faster start-up, a smoother desktop and less background work.", "Tez start-up, smooth desktop aur kam background kaam."),
    gaming: t("Less input lag and fewer interruptions while you play.", "Game khelte waqt kam lag aur kam rukawat."),
    gpu: t("Graphics card and display settings.", "Graphics card aur display ki settings."),
    power: t("Battery and power settings for longer use.", "Battery aur power ki settings, zyada chalne ke liye."),
    privacy: t("Limit tracking, telemetry and ad IDs.", "Tracking, telemetry aur ad IDs ko seemit karein."),
    ai: t("Built-in Windows AI features and their background work.", "Windows ke built-in AI features aur unka background kaam."),
    services: t("Background Windows services you may not need.", "Background Windows services jo shayad zaroori na hon."),
    explorer: t("How File Explorer looks and behaves.", "File Explorer kaisa dikhe aur chale."),
    taskbar: t("Taskbar icons, search and extras.", "Taskbar ke icons, search aur extras."),
    system: t("General Windows behaviour and upkeep.", "Windows ka general behaviour aur upkeep."),
  });

  onMount(load);
  async function load() {
    try { tweaks = await api.tweaks(); } catch (e) { toast(String(e), "warn"); tweaks = []; }
  }

  const maxRisk = $derived(ui.settings?.trust_level ?? "ask");
  const allowed = (r: string) => maxRisk === "expert" || (maxRisk === "ask" ? r !== "expert" : r === "safe");
  const inTab = (id: Tab, x: TweakState) => (id === "recommended" ? x.recommended : x.category === id);

  const visible = $derived.by(() => {
    if (!tweaks) return [];
    const q = query.trim().toLowerCase();
    return tweaks
      .filter((tw) => allowed(tw.risk))
      .filter((tw) => (q ? (tw.name + " " + tw.description).toLowerCase().includes(q) : inTab(tab, tw)));
  });

  // Per-tab "applied / total" over the tweaks the trust level allows.
  const stats = $derived.by(() => {
    const out = {} as Record<Tab, { done: number; total: number }>;
    for (const [id] of TABS) {
      const items = (tweaks ?? []).filter((x) => allowed(x.risk) && inTab(id, x));
      out[id] = { done: items.filter((x) => x.applied).length, total: items.length };
    }
    return out;
  });

  const todo = $derived((tweaks ?? []).filter((tw) => tw.recommended && tw.available && !tw.applied && allowed(tw.risk)));
  const appliedCount = $derived((tweaks ?? []).filter((tw) => tw.applied).length);
  const needsAdmin = $derived(todo.some((tw) => tw.needs_admin) && !ui.info?.is_admin);
  // Not running as admin (only when the app actually reports that).
  const notAdmin = $derived(ui.info?.is_admin === false);
  // Available tweaks that need admin and are not applied yet; drives the banner.
  const adminWaiting = $derived((tweaks ?? []).filter((tw) => tw.needs_admin && tw.available && !tw.applied).length);
  // Turning ON a tweak that needs admin is blocked until the app runs as admin.
  const blocked = (tw: TweakState) => tw.needs_admin && notAdmin && !tw.applied;

  async function toggle(tw: TweakState, on: boolean) {
    busy = tw.id;
    try {
      const note = await api.tweakSet(tw.id, on);
      restoreNote(note);
      toast(on ? t(`Applied: ${tw.name}`, `Lag gaya: ${tw.name}`) : t(`Undone: ${tw.name}`, `Wapas liya: ${tw.name}`));
      if (on && tw.restart) toast(restartText(tw.restart), "info");
    } catch (e) {
      const msg = String(e);
      toast(msg.includes("administrator") ? t("Needs administrator: restart Sweeplark as admin (Startup page).", "Admin chahiye: Sweeplark ko admin mode mein kholein.") : msg, "warn");
    } finally {
      busy = null;
      await load();
    }
  }

  function restoreNote(note: string | null) {
    if (!note) return;
    if (note === "restore_point_ok") toast(t("Restore point created first, just in case.", "Pehle ek restore point bana liya, ehtiyaat ke liye."), "info");
    else toast(note, "warn");
  }

  async function applyAll() {
    confirm = false;
    const ids = todo.map((x) => x.id);
    const r = await runJob("tweaks", t("Applying optimizations", "Optimizations laga raha hoon"), "optimizer", () => api.tweaksApply(ids));
    if (r) {
      restoreNote(r.restore_point);
      toast(t(`${r.applied.length} optimizations applied`, `${r.applied.length} optimizations lag gaye`));
      if (r.skipped.length) toast(t(`${r.skipped.length} skipped (some need administrator)`, `${r.skipped.length} chhode gaye (kuch ke liye admin chahiye)`), "warn");
    }
    await load();
  }

  function restartText(r: string) {
    return r === "reboot" ? t("Takes effect after a restart.", "Restart ke baad asar hoga.")
      : r === "signout" ? t("Takes effect after signing out.", "Sign out ke baad asar hoga.")
      : t("Restart Explorer (Tools) to see it.", "Dekhne ke liye Explorer restart karein (Tools).");
  }
  const RESTART = $derived<Record<string, string>>({ reboot: t("Restart", "Restart"), signout: t("Sign out", "Sign out"), explorer: "Explorer" });
</script>

<header class="head">
  <div><h1>{t("Optimizer", "Optimizer")}</h1>
    <p class="muted">{t("Windows tweaks for speed, gaming, privacy and a cleaner desktop. Every change can be switched back off.", "Speed, gaming, privacy aur saaf desktop ke liye Windows tweaks. Har badlav wapas band ho sakta hai.")}</p></div>
  <input type="text" class="search" placeholder={t("Search tweaks…", "Tweaks dhoondein…")} bind:value={query} />
</header>

<JobProgress id="tweaks" hint={t("Changing Windows settings one by one…", "Windows settings ek-ek karke badal raha hoon…")} />

{#if tweaks}
  <AdminBanner count={adminWaiting} text={t("Restart Sweeplark as administrator to apply these. You come back to this page; nothing is lost.", "Inhe lagane ke liye Sweeplark ko admin mode mein kholein. Aap isi page par wapas aayenge; kuch nahi khoyega.")} />

  <section class="card hero">
    <div class="col hero-text">
      <h2>{todo.length ? t(`${todo.length} recommended optimizations ready`, `${todo.length} recommended optimizations taiyaar`) : t("All recommended optimizations are on", "Saare recommended optimizations chalu hain")}</h2>
      <p class="muted small">{t(`${appliedCount} of ${tweaks.length} tweaks active on this PC.`, `Is PC par ${tweaks.length} mein se ${appliedCount} tweaks chalu.`)}
        {#if needsAdmin}{t(" Some need administrator rights.", " Kuch ke liye admin chahiye.")}{/if}</p>
    </div>
    <button class="btn primary" disabled={!todo.length || applying} onclick={() => (confirm = true)}>
      {#if applying}<div class="spin" style="border-top-color:#fff"></div>{:else}<Icon name="sparkle" size={17} />{/if}{t("Apply recommended", "Recommended lagao")}</button>
  </section>

  <nav class="tabs" aria-label={t("Categories", "Categories")}>
    {#each TABS as [id, label, icon]}
      <button class:on={tab === id && !query} onclick={() => { tab = id; query = ""; }}><Icon name={icon} size={15} />{label}
        <em class="num">{stats[id].done}/{stats[id].total}</em></button>
    {/each}
  </nav>
  <p class="hint small muted">{query ? t("Showing search results. Clear the search to go back to the tabs.", "Search ke natije dikh rahe hain. Tabs par wapas jaane ke liye search saaf karein.") : TAB_HINT[tab]}</p>

  <div class="card list">
    {#each visible as tw (tw.id)}
      <div class="item" class:dim={!tw.available}>
        <div class="col grow" style="gap:3px">
          <div class="row wrap"><strong>{tw.name}</strong>
            {#if tw.risk !== "safe"}<span class="chip {tw.risk}">{tw.risk === "ask" ? t("Moderate", "Moderate") : "Expert"}</span>{/if}
            {#if tw.restart}<span class="chip">{RESTART[tw.restart]}</span>{/if}
            {#if tw.needs_admin}<span class="chip" title={blocked(tw) ? t("Needs administrator. Restart Sweeplark as admin to turn this on.", "Admin chahiye. Ise chalu karne ke liye Sweeplark ko admin mode mein kholein.") : t("Needs administrator", "Admin chahiye")}><Icon name="lock" size={11} />{t("Admin", "Admin")}</span>{/if}
            {#if !tw.available}<span class="chip">{t("Not for this PC", "Is PC ke liye nahi")}</span>{/if}
          </div>
          <span class="muted small">{tw.description}</span>
          {#if tw.note}<span class="note small"><Icon name="alert" size={13} />{tw.note}</span>{/if}
        </div>
        {#if busy === tw.id}<div class="spin"></div>{/if}
        <Toggle checked={tw.applied} label={tw.name} disabled={!tw.available || busy !== null || blocked(tw)} onchange={(v) => toggle(tw, v)} />
      </div>
    {:else}
      <div class="empty">{t("Nothing here for your trust level. Change it in Settings to see more.", "Aapke trust level par yahan kuch nahi. Settings mein badal kar aur dekhein.")}</div>
    {/each}
  </div>
  <p class="muted small credit">{t("Tweak values follow Microsoft documentation and the open-source optimizerDuck project.", "Tweak values Microsoft docs aur open-source optimizerDuck project par aadharit hain.")}</p>
{:else}
  <div class="row"><div class="spin"></div><span class="muted">{t("Reading Windows settings…", "Windows settings padh raha hoon…")}</span></div>
{/if}

<Modal bind:open={confirm} title={t("Apply recommended optimizations?", "Recommended optimizations lagayein?")} wide>
  <p class="muted" style="margin-bottom:10px">{t("Each one is recorded in History and can be switched off again.", "Har ek Hisaab mein likha jayega aur wapas band ho sakta hai.")}
    {#if ui.info?.is_admin && ui.settings?.restore_point_before_tweaks}{t(" A Windows restore point is made first.", " Pehle ek Windows restore point banega.")}{/if}</p>
  <div class="list">{#each todo as tw}<div class="item"><Icon name="check" size={15} /><span>{tw.name}</span><span class="spacer"></span>
    {#if tw.restart}<span class="chip">{RESTART[tw.restart]}</span>{/if}{#if tw.risk !== "safe"}<span class="chip ask">{t("Moderate", "Moderate")}</span>{/if}</div>{/each}</div>
  {#snippet footer()}
    <button class="btn" onclick={() => (confirm = false)}>{t("Cancel", "Ruko")}</button>
    <button class="btn primary" onclick={applyAll}>{t(`Apply ${todo.length}`, `${todo.length} lagao`)}</button>
  {/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .head h1 { margin-bottom: 4px; }
  .search { width: 260px; }
  .hero { display: flex; align-items: center; gap: 16px; margin-bottom: 14px; padding: 16px 18px; flex-wrap: wrap; }
  .hero-text { gap: 4px; flex: 1; min-width: 240px; }
  .tabs { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .tabs button { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border-radius: 999px; border: 1px solid var(--line); background: var(--surface); color: var(--ink-2); cursor: pointer; font-weight: 520; }
  .tabs button:hover { background: var(--surface-3); }
  .tabs button.on { background: var(--teal); border-color: var(--teal); color: #fff; }
  .tabs em { font-style: normal; font-size: 11px; font-weight: 600; opacity: .8; padding-left: 4px; border-left: 1px solid currentColor; line-height: 12px; }
  .tabs button:not(.on) em { opacity: .6; }
  .hint { margin: 0 4px 12px; }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .dim { opacity: .55; }
  .note { display: inline-flex; gap: 5px; align-items: center; color: var(--amber-text); }
  .credit { margin-top: 10px; text-align: center; }
</style>
