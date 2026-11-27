<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Modal from "../components/Modal.svelte";
  import { api } from "../lib/api";
  import { t, toast, ui } from "../lib/state.svelte";
  import type { TweakCategory, TweakState } from "../lib/types";

  let tweaks = $state<TweakState[] | null>(null);
  let tab = $state<TweakCategory | "recommended">("recommended");
  let busy = $state<string | null>(null);
  let query = $state("");
  let confirm = $state(false);
  let applying = $state(false);

  const TABS = $derived<[TweakCategory | "recommended", string, string][]>([
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

  onMount(load);
  async function load() {
    try { tweaks = await api.tweaks(); } catch (e) { toast(String(e), "warn"); tweaks = []; }
  }

  const maxRisk = $derived(ui.settings?.trust_level ?? "ask");
  const allowed = (r: string) => maxRisk === "expert" || (maxRisk === "ask" ? r !== "expert" : r === "safe");
  const visible = $derived.by(() => {
    if (!tweaks) return [];
    const q = query.trim().toLowerCase();
    return tweaks
      .filter((tw) => allowed(tw.risk))
      .filter((tw) => (q ? (tw.name + " " + tw.description).toLowerCase().includes(q) : tab === "recommended" ? tw.recommended : tw.category === tab));
  });
  const todo = $derived((tweaks ?? []).filter((tw) => tw.recommended && tw.available && !tw.applied && allowed(tw.risk)));
  const appliedCount = $derived((tweaks ?? []).filter((tw) => tw.applied).length);
  const needsAdmin = $derived(todo.some((tw) => tw.needs_admin) && !ui.info?.is_admin);

  async function toggle(tw: TweakState, on: boolean) {
    busy = tw.id;
    try {
      await api.tweakSet(tw.id, on);
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

  async function applyAll() {
    confirm = false;
    applying = true;
    try {
      const r = await api.tweaksApply(todo.map((x) => x.id));
      toast(t(`${r.applied.length} optimizations applied`, `${r.applied.length} optimizations lag gaye`));
      if (r.skipped.length) toast(t(`${r.skipped.length} skipped (some need administrator)`, `${r.skipped.length} chhode gaye (kuch ke liye admin chahiye)`), "warn");
    } catch (e) { toast(String(e), "warn"); }
    applying = false;
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

{#if tweaks}
  <section class="card hero">
    <div class="col" style="gap:4px">
      <h2>{todo.length ? t(`${todo.length} recommended optimizations ready`, `${todo.length} recommended optimizations taiyaar`) : t("All recommended optimizations are on", "Saare recommended optimizations chalu hain")}</h2>
      <p class="muted small">{t(`${appliedCount} of ${tweaks.length} tweaks active on this PC.`, `Is PC par ${tweaks.length} mein se ${appliedCount} tweaks chalu.`)}
        {#if needsAdmin}{t(" Some need administrator rights.", " Kuch ke liye admin chahiye.")}{/if}</p>
    </div>
    <span class="spacer"></span>
    {#if needsAdmin}<button class="btn" onclick={() => api.relaunchAdmin()}><Icon name="lock" size={16} />{t("Restart as admin", "Admin mode")}</button>{/if}
    <button class="btn primary" disabled={!todo.length || applying} onclick={() => (confirm = true)}>
      {#if applying}<div class="spin" style="border-top-color:#fff"></div>{:else}<Icon name="sparkle" size={17} />{/if}{t("Apply recommended", "Recommended lagao")}</button>
  </section>

  <nav class="tabs">
    {#each TABS as [id, label, icon]}
      <button class:on={tab === id && !query} onclick={() => { tab = id; query = ""; }}><Icon name={icon} size={15} />{label}
        <em>{(tweaks ?? []).filter((x) => (id === "recommended" ? x.recommended : x.category === id) && x.applied).length}</em></button>
    {/each}
  </nav>

  <div class="card list">
    {#each visible as tw (tw.id)}
      <div class="item" class:dim={!tw.available}>
        <div class="col grow" style="gap:3px">
          <div class="row wrap"><strong>{tw.name}</strong>
            {#if tw.risk !== "safe"}<span class="chip {tw.risk}">{tw.risk === "ask" ? t("Moderate", "Moderate") : "Expert"}</span>{/if}
            {#if tw.restart}<span class="chip">{RESTART[tw.restart]}</span>{/if}
            {#if tw.needs_admin}<span class="chip"><Icon name="lock" size={11} />Admin</span>{/if}
            {#if !tw.available}<span class="chip">{t("Not for this PC", "Is PC ke liye nahi")}</span>{/if}
          </div>
          <span class="muted small">{tw.description}</span>
          {#if tw.note}<span class="note small"><Icon name="alert" size={13} />{tw.note}</span>{/if}
        </div>
        {#if busy === tw.id}<div class="spin"></div>{/if}
        <Toggle checked={tw.applied} label={tw.name} disabled={!tw.available || busy !== null} onchange={(v) => toggle(tw, v)} />
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
  <p class="muted" style="margin-bottom:10px">{t("Each one is recorded in History and can be switched off again.", "Har ek Hisaab mein likha jayega aur wapas band ho sakta hai.")}</p>
  <div class="list">{#each todo as tw}<div class="item"><Icon name="check" size={15} /><span>{tw.name}</span><span class="spacer"></span>
    {#if tw.restart}<span class="chip">{RESTART[tw.restart]}</span>{/if}{#if tw.risk !== "safe"}<span class="chip ask">{t("Moderate", "Moderate")}</span>{/if}</div>{/each}</div>
  {#snippet footer()}
    <button class="btn" onclick={() => (confirm = false)}>{t("Cancel", "Ruko")}</button>
    <button class="btn primary" onclick={applyAll}>{t(`Apply ${todo.length}`, `${todo.length} lagao`)}</button>
  {/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .search { width: 260px; }
  .hero { display: flex; align-items: center; gap: 14px; margin-bottom: 14px; flex-wrap: wrap; }
  .tabs { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 12px; }
  .tabs button { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border-radius: 999px; border: 1px solid var(--line); background: var(--surface); color: var(--ink-2); cursor: pointer; font-weight: 520; }
  .tabs button.on { background: var(--teal); border-color: var(--teal); color: #fff; }
  .tabs em { font-style: normal; font-size: 11px; opacity: .7; }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .dim { opacity: .55; }
  .note { display: inline-flex; gap: 5px; align-items: center; color: var(--amber-text); }
  .credit { margin-top: 10px; text-align: center; }
</style>
