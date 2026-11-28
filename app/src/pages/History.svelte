<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { api } from "../lib/api";
  import { ago, bytes } from "../lib/format";
  import { toast, t } from "../lib/state.svelte";
  import type { ChangeRecord, History } from "../lib/types";

  type Filter = "all" | "clean" | "optimize" | "startup" | "boost";
  const FILTERS: [Filter, string][] = [
    ["all", t("All", "Sab")],
    ["clean", t("Cleaning", "Safai")],
    ["optimize", t("Optimizer", "Optimizer")],
    ["startup", t("Startup", "Startup")],
    ["boost", t("Boost", "Boost")],
  ];

  let data = $state<History | null>(null);
  let filter = $state<Filter>("all");
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

  function iconFor(c: string): string {
    if (c === "clean") return "broom";
    if (c === "tweak" || c === "tweak.off") return "sparkle";
    if (c === "startup") return "leaf";
    if (c === "boost" || c === "optimize") return "rocket";
    return "history";
  }
  function filterOf(c: string): Filter | "other" {
    if (c === "clean" || c === "optimize" || c === "startup" || c === "boost") return c;
    return "other";
  }

  const dayKey = (d: Date) => `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
  function dayLabel(iso: string): string {
    const d = new Date(iso);
    const now = new Date();
    const yest = new Date(now); yest.setDate(now.getDate() - 1);
    if (dayKey(d) === dayKey(now)) return t("Today", "Aaj");
    if (dayKey(d) === dayKey(yest)) return t("Yesterday", "Kal");
    return d.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  }

  const groups = $derived.by(() => {
    const rows = (data?.rows ?? []).filter((r) => filter === "all" || filterOf(r.category) === filter);
    const out: { key: string; label: string; freed: number; rows: ChangeRecord[] }[] = [];
    const byKey = new Map<string, (typeof out)[number]>();
    for (const r of rows) {
      const d = new Date(r.at);
      const k = dayKey(d);
      let g = byKey.get(k);
      if (!g) {
        g = { key: k, label: dayLabel(r.at), freed: 0, rows: [] };
        byKey.set(k, g);
        out.push(g);
      }
      g.rows.push(r);
      g.freed += r.bytes_freed || 0;
    }
    return out;
  });
</script>

<header class="head"><div><h1>{t("History", "Hisaab")}</h1><p class="muted">{t("Everything Sweeplark changed is listed here, with undo where possible.", "Sweeplark ne jo bhi badla, sab yahan hai. Jo wapas ho sakta hai, uska undo bhi yahin hai.")}</p></div></header>

{#if data}
  <section class="sum">
    <div class="card stat"><strong class="num">{bytes(data.summary.bytes_freed)}</strong><span class="muted small">{t("freed in total", "kul jagah bachi")}</span></div>
    <div class="card stat"><strong class="num">{data.summary.changes}</strong><span class="muted small">{t("changes", "badlav")}</span></div>
    <div class="card stat"><strong class="num">{data.summary.undoable}</strong><span class="muted small">{t("can be undone", "undo ho sakte hain")}</span></div>
  </section>

  <div class="filters" role="group" aria-label={t("Filter changes", "Badlav filter karein")}>
    {#each FILTERS as [id, label]}
      <button class="fchip" class:on={filter === id} aria-pressed={filter === id} onclick={() => (filter = id)}>{label}</button>
    {/each}
  </div>

  {#each groups as g (g.key)}
    <section class="day">
      <div class="day-head"><h2>{g.label}</h2><span class="muted small num">{t(`${g.rows.length} changes`, `${g.rows.length} badlav`)}{g.freed ? ` · ${bytes(g.freed)} ${t("freed", "bachi")}` : ""}</span></div>
      <div class="card list">
        {#each g.rows as r (r.id)}
          <div class="item" class:reverted={r.status === "reverted"}>
            <span class="ic"><Icon name={iconFor(r.category)} size={17} /></span>
            <div class="col grow" style="gap:2px">
              <strong>{r.title}</strong>
              <span class="muted small">{ago(r.at)} · #{r.id}{r.status === "reverted" ? t(" · undone", " · wapas liya gaya") : ""}{r.category === "clean" ? t(" · deleted cache can't be restored", " · cache wapas nahi aata") : ""}</span>
            </div>
            {#if r.bytes_freed}<span class="num freed">{bytes(r.bytes_freed)}</span>{/if}
            {#if r.can_undo}<button class="btn small" onclick={() => undo(r.id)}><Icon name="undo" size={15} />{t("Undo", "Undo")}</button>{/if}
          </div>
        {/each}
      </div>
    </section>
  {:else}
    <div class="card empty">
      <Icon name="history" size={28} />
      <p>{data.rows.length ? t("Nothing in this filter.", "Is filter mein kuch nahi.") : t("No changes yet.", "Abhi tak koi badlav nahi.")}</p>
    </div>
  {/each}
{/if}

<style>
  .head { margin-bottom: 20px; }
  .head p { margin-top: 4px; }
  .sum { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 18px; }
  .stat { display: flex; flex-direction: column; gap: 2px; padding: 18px 20px; }
  .stat strong { font-size: 24px; font-weight: 650; }

  .filters { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 22px; }
  .fchip { height: 32px; padding: 0 14px; border-radius: 999px; border: 1px solid var(--line-strong); background: var(--surface); color: var(--ink-2); cursor: pointer; font-weight: 520; font-size: 13px; }
  .fchip:hover { background: var(--surface-3); }
  .fchip.on { background: var(--teal-soft); border-color: var(--teal); color: var(--teal-text); font-weight: 600; }
  .fchip:focus-visible { outline: 2px solid var(--teal); outline-offset: 2px; }

  .day { margin-bottom: 22px; }
  .day-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin: 0 4px 8px; }
  .day-head h2 { font-size: 14px; font-weight: 600; color: var(--ink-2); }
  .card.list { padding: 4px 16px; }
  .grow { flex: 1; min-width: 0; }
  .ic { width: 32px; height: 32px; border-radius: 10px; display: grid; place-items: center; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .freed { font-weight: 600; color: var(--good); white-space: nowrap; }
  .reverted { opacity: .55; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 8px; }
  .empty p { margin: 0; }
</style>
