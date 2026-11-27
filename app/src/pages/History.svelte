<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { api } from "../lib/api";
  import { ago, bytes } from "../lib/format";
  import { toast , t } from "../lib/state.svelte";
  import type { History } from "../lib/types";

  let data = $state<History | null>(null);
  onMount(load);
  async function load() { data = await api.history(200); }

  async function undo(id: number) {
    try {
      const r = await api.undo(id);
      toast(t(`Undone: ${r.title}`, `Wapas le liya: ${r.title}`));
      await load();
    } catch (e) {
      toast(String(e), "warn");
    }
  }
  const ICON: Record<string, string> = { clean: "broom", startup: "rocket", optimize: "leaf" };
</script>

<header class="head"><div><h1>{t("History", "Hisaab")}</h1><p class="muted">{t("Everything Sweeplark changed is listed here, with undo where possible.", "Sweeplark ne jo bhi badla, sab yahan hai. Jo wapas ho sakta hai, uska undo bhi yahin hai.")}</p></div></header>

{#if data}
  <section class="sum">
    <div class="card stat"><strong class="num">{bytes(data.summary.bytes_freed)}</strong><span class="muted">{t("freed in total", "kul jagah bachi")}</span></div>
    <div class="card stat"><strong class="num">{data.summary.changes}</strong><span class="muted">{t("changes", "badlav")}</span></div>
    <div class="card stat"><strong class="num">{data.summary.undoable}</strong><span class="muted">{t("can be undone", "undo ho sakte hain")}</span></div>
  </section>
  <div class="card list">
    {#each data.rows as r (r.id)}
      <div class="item" class:reverted={r.status === "reverted"}>
        <span class="ic"><Icon name={ICON[r.category] ?? "info"} size={17} /></span>
        <div class="col grow" style="gap:2px">
          <strong>{r.title}</strong>
          <span class="muted small">{ago(r.at)} · #{r.id}{r.status === "reverted" ? t(" · undone", " · wapas liya gaya") : ""}{r.category === "clean" ? t(" · deleted cache can't be restored", " · cache wapas nahi aata") : ""}</span>
        </div>
        {#if r.bytes_freed}<span class="num">{bytes(r.bytes_freed)}</span>{/if}
        {#if r.can_undo}<button class="btn small" onclick={() => undo(r.id)}><Icon name="undo" size={15} />Undo</button>{/if}
      </div>
    {:else}
      <div class="empty">{t("No changes yet.", "Abhi tak koi badlav nahi.")}</div>
    {/each}
  </div>
{/if}

<style>
  .head { margin-bottom: 18px; }
  .sum { display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; margin-bottom: 14px; }
  .stat { display: flex; flex-direction: column; gap: 2px; }
  .stat strong { font-size: 24px; font-weight: 650; }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .ic { width: 32px; height: 32px; border-radius: 10px; display: grid; place-items: center; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .reverted { opacity: .55; }
</style>
