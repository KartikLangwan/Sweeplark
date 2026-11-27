<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { api } from "../lib/api";
  import { toast, ui } from "../lib/state.svelte";
  import type { StartupItem } from "../lib/types";

  let items = $state<StartupItem[] | null>(null);
  let busy = $state<string | null>(null);

  onMount(load);
  async function load() {
    items = await api.startupList();
  }

  const on = $derived(items?.filter((i) => i.enabled).length ?? 0);
  const suggest = $derived(items?.filter((i) => i.enabled && i.advice === "disable") ?? []);

  async function set(item: StartupItem, enabled: boolean) {
    busy = item.id;
    try {
      await api.startupSet(item.id, enabled);
      toast(`${item.name} ${enabled ? "chalu ho gaya" : "band ho gaya"}. Hisaab mein undo kar sakte hain.`);
      await load();
    } catch (e) {
      toast(String(e).includes("administrator") ? `${item.name} sab users ke liye hai: admin chahiye` : String(e), "warn");
      await load();
    } finally {
      busy = null;
    }
  }
</script>

<header class="head">
  <div><h1>Startup apps</h1><p class="muted">Jo apps PC on hote hi chalti hain. Band karne se kuch delete nahi hota: Task Manager jaisa hi on/off, aur undo bhi.</p></div>
</header>

{#if items}
  <section class="summary">
    <div class="card stat"><strong class="num">{on}</strong><span class="muted">chalu</span></div>
    <div class="card stat"><strong class="num">{items.length - on}</strong><span class="muted">band</span></div>
    <div class="card tip">
      <Icon name="leaf" />
      <p>{#if suggest.length}{suggest.length} apps band karna safe hai: <strong>{suggest.map((s) => s.name).join(", ")}</strong>. Zarurat par aap inhe khud khol sakte hain.{:else}Startup achha hai. Koi bhari app nahi mili.{/if}</p>
    </div>
  </section>

  <div class="card list">
    {#each items as item (item.id)}
      <div class="item">
        <span class="app-ic" class:off={!item.enabled}>{item.name.slice(0, 1).toUpperCase()}</span>
        <div class="col grow" style="gap:3px">
          <div class="row"><strong>{item.name}</strong>
            {#if item.advice === "disable"}<span class="chip info">Band kar sakte hain</span>{/if}
            {#if item.advice === "keep"}<span class="chip good">Chalu rakhein</span>{/if}
            {#if item.needs_admin}<span class="chip" title="Sab users ke liye: admin chahiye"><Icon name="lock" size={12} />Admin</span>{/if}
          </div>
          <span class="muted small">{item.advice_note ?? item.source_label}</span>
          <code class="selectable" title={item.command}>{item.command}</code>
        </div>
        {#if busy === item.id}<div class="spin"></div>{/if}
        <Toggle checked={item.enabled} label={item.name} disabled={busy !== null || (item.needs_admin && !ui.info?.is_admin)}
          onchange={(v) => set(item, v)} />
      </div>
    {:else}
      <div class="empty">Koi startup app nahi mili.</div>
    {/each}
  </div>
  {#if items.some((i) => i.needs_admin) && !ui.info?.is_admin}
    <p class="muted small adm"><Icon name="lock" size={14} />"Admin" wali apps sab users ke liye hain. Unhe badalne ke liye
      <button class="link" onclick={() => api.relaunchAdmin()}>Sweeplark ko admin mode mein kholein</button>.</p>
  {/if}
{:else}
  <div class="row"><div class="spin"></div><span class="muted">Startup apps dhoond raha hoon…</span></div>
{/if}

<style>
  .head { margin-bottom: 18px; }
  .summary { display: grid; grid-template-columns: 120px 120px 1fr; gap: 14px; margin-bottom: 14px; }
  .stat { display: flex; flex-direction: column; gap: 2px; padding: 14px 16px; }
  .stat strong { font-size: 26px; font-weight: 650; }
  .tip { display: flex; align-items: center; gap: 12px; color: var(--teal-text); background: linear-gradient(120deg, var(--surface), var(--teal-soft)); }
  .tip p { color: var(--ink); }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .app-ic { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; font-weight: 700; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .app-ic.off { background: var(--surface-3); color: var(--slate); }
  code { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 11.5px; color: var(--slate); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 640px; }
  .adm { display: flex; align-items: center; gap: 6px; margin-top: 10px; }
  .link { background: none; border: 0; padding: 0; color: var(--teal-text); font-weight: 600; cursor: pointer; text-decoration: underline; }
</style>
