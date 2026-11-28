<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import AdminBanner from "../components/AdminBanner.svelte";
  import { api } from "../lib/api";
  import { toast, ui , t } from "../lib/state.svelte";
  import type { StartupItem } from "../lib/types";

  type Filter = "all" | "on" | "off" | "suggest";

  let items = $state<StartupItem[] | null>(null);
  let busy = $state<string | null>(null);
  let query = $state("");
  let filter = $state<Filter>("all");

  onMount(load);
  async function load() {
    try { items = await api.startupList(); }
    catch (e) { toast(String(e), "warn"); items = []; }
  }

  const on = $derived(items?.filter((i) => i.enabled).length ?? 0);
  const suggest = $derived(items?.filter((i) => i.enabled && i.advice === "disable") ?? []);
  const needAdmin = $derived(items?.filter((i) => i.needs_admin).length ?? 0);

  const shown = $derived.by(() => {
    if (!items) return [] as StartupItem[];
    const q = query.trim().toLowerCase();
    return items.filter((i) => {
      if (filter === "on" && !i.enabled) return false;
      if (filter === "off" && i.enabled) return false;
      if (filter === "suggest" && !(i.enabled && i.advice === "disable")) return false;
      if (!q) return true;
      return i.name.toLowerCase().includes(q) || i.command.toLowerCase().includes(q) || i.source_label.toLowerCase().includes(q);
    });
  });

  const filters = $derived<{ id: Filter; label: string; count: number }[]>([
    { id: "all", label: t("All", "Sab"), count: items?.length ?? 0 },
    { id: "on", label: t("On", "Chalu"), count: on },
    { id: "off", label: t("Off", "Band"), count: (items?.length ?? 0) - on },
    { id: "suggest", label: t("Suggested to turn off", "Band karne ka suggest"), count: suggest.length },
  ]);

  async function set(item: StartupItem, enabled: boolean) {
    busy = item.id;
    try {
      await api.startupSet(item.id, enabled);
      toast(enabled ? t(`${item.name} turned on. You can undo it in History.`, `${item.name} chalu ho gaya. Hisaab mein undo kar sakte hain.`) : t(`${item.name} turned off. You can undo it in History.`, `${item.name} band ho gaya. Hisaab mein undo kar sakte hain.`));
      await load();
    } catch (e) {
      toast(String(e).includes("administrator") ? t(`${item.name} is set for all users: administrator needed`, `${item.name} sab users ke liye hai: admin chahiye`) : String(e), "warn");
      await load();
    } finally {
      busy = null;
    }
  }
</script>

<header class="head">
  <div class="titles">
    <h1>{t("Startup apps", "Startup apps")}</h1>
    <p class="muted">{t("Apps that start with your PC. Turning one off deletes nothing: it works like Task Manager, with undo.", "Jo apps PC on hote hi chalti hain. Band karne se kuch delete nahi hota: Task Manager jaisa hi on/off, aur undo bhi.")}</p>
  </div>
</header>

{#if items}
  <section class="summary">
    <div class="card stat"><strong class="num">{on}</strong><span class="muted">{t("on", "chalu")}</span></div>
    <div class="card stat"><strong class="num">{items.length - on}</strong><span class="muted">{t("off", "band")}</span></div>
    <div class="card stat"><strong class="num">{suggest.length}</strong><span class="muted">{t("suggested to turn off", "band karne ka suggest")}</span></div>
    <div class="card tip">
      <span class="tip-ic"><Icon name="leaf" /></span>
      <p>{#if suggest.length}{t(`Safe to turn off (${suggest.length}):`, `${suggest.length} apps band karna safe hai:`)} <strong>{suggest.map((s) => s.name).join(", ")}</strong>. {t("You can still open them yourself.", "Zarurat par aap inhe khud khol sakte hain.")}{:else}{t("Startup looks good. No heavy apps found.", "Startup achha hai. Koi bhari app nahi mili.")}{/if}</p>
    </div>
  </section>

  {#if needAdmin}
    <AdminBanner count={needAdmin} text={t("Some startup items apply to all users, so they need administrator rights to change. Restart Sweeplark as admin to change them. You come back to this page.", "Kuch startup cheezein sab users ke liye hain, unhe badalne ke liye admin rights chahiye. Sweeplark ko admin mode mein kholein. Aap isi page par wapas aayenge.")} />
  {/if}

  <div class="toolbar">
    <input type="text" class="search" bind:value={query} placeholder={t("Search apps or commands", "App ya command dhoondhein")} aria-label={t("Search startup items", "Startup items dhoondhein")} />
    <div class="filters" role="group" aria-label={t("Filter", "Filter")}>
      {#each filters as f (f.id)}
        <button class="btn small" class:primary={filter === f.id} aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>
          {f.label} <span class="num count">{f.count}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="card list">
    {#each shown as item (item.id)}
      <div class="item">
        <span class="app-ic" class:off={!item.enabled}>{item.name.slice(0, 1).toUpperCase()}</span>
        <div class="col grow" style="gap:4px">
          <div class="row wrap"><strong>{item.name}</strong>
            <span class="chip">{item.source_label}</span>
            {#if item.advice === "disable"}<span class="chip info">{t("Can turn off", "Band kar sakte hain")}</span>{/if}
            {#if item.advice === "keep"}<span class="chip good">{t("Keep on", "Chalu rakhein")}</span>{/if}
            {#if item.needs_admin}<span class="chip" title={t("For all users: administrator needed", "Sab users ke liye: admin chahiye")}><Icon name="lock" size={12} />Admin</span>{/if}
          </div>
          {#if item.advice_note}<span class="muted small">{item.advice_note}</span>{/if}
          <code class="selectable" title={item.command}>{item.command}</code>
        </div>
        {#if busy === item.id}<div class="spin"></div>{/if}
        <Toggle controlled checked={item.enabled} label={item.name} disabled={busy !== null || (item.needs_admin && !ui.info?.is_admin)}
          onchange={(v) => set(item, v)} />
      </div>
    {:else}
      <div class="empty">
        {#if items.length}
          {t("Nothing matches your search or filter.", "Aapki search ya filter se kuch nahi mila.")}
        {:else}
          {t("No startup apps found.", "Koi startup app nahi mili.")}
        {/if}
      </div>
    {/each}
  </div>
{:else}
  <div class="card loading">
    <div class="spin"></div>
    <div class="col" style="gap:2px">
      <strong>{t("Finding startup apps…", "Startup apps dhoond raha hoon…")}</strong>
      <span class="muted small">{t("This can take a few seconds, since scheduled tasks are checked too.", "Isme kuch seconds lag sakte hain, scheduled tasks bhi dekhte hain.")}</span>
    </div>
  </div>
{/if}

<style>
  .head { display: flex; align-items: flex-end; margin-bottom: 20px; }
  .titles { display: flex; flex-direction: column; gap: 4px; }
  .summary { display: grid; grid-template-columns: 110px 110px 150px minmax(0, 1fr); gap: 14px; margin-bottom: 16px; }
  .stat { display: flex; flex-direction: column; gap: 2px; padding: 14px 16px; }
  .stat strong { font-size: 26px; font-weight: 650; }
  .stat .muted { font-size: 12.5px; }
  .tip { display: flex; align-items: center; gap: 12px; }
  .tip p { color: var(--ink); }
  .tip-ic { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 11px; background: var(--teal-soft); color: var(--teal-text); flex: none; }

  .toolbar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; margin-bottom: 12px; }
  .search { flex: 1 1 260px; min-width: 200px; max-width: 420px; }
  .filters { display: flex; gap: 6px; flex-wrap: wrap; }
  .count { font-size: 11.5px; opacity: .75; margin-left: 2px; }

  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .app-ic { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; font-weight: 700; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .app-ic.off { background: var(--surface-3); color: var(--slate); }
  code { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 11.5px; color: var(--slate); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; display: block; }
  .list > .item { padding: 14px 4px; }

  .loading { display: flex; align-items: center; gap: 14px; padding: 22px 24px; }
  @media (max-width: 760px) { .summary { grid-template-columns: 1fr 1fr; } .tip { grid-column: 1 / -1; } }
</style>
