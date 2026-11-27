<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { api, on } from "../lib/api";
  import { ago, bytes, shortPath } from "../lib/format";
  import { refreshInbox, toast , t } from "../lib/state.svelte";
  import type { Approval, Message } from "../lib/types";

  let messages = $state<Message[]>([]);
  let approvals = $state<Approval[]>([]);
  let ticks = $state<Record<string, boolean>>({});
  let busy = $state<number | null>(null);

  async function load() {
    const box = await refreshInbox();
    messages = box.messages;
    approvals = await api.approvals();
  }

  onMount(() => {
    load().then(() => api.markRead(null)).then(refreshInbox);
    const u = on("inbox-changed", load);
    return () => { u.then((f) => f()); };
  });

  async function decide(a: Approval, approve: boolean) {
    busy = a.id;
    try {
      const files = a.detail.risky_files.filter((f) => ticks[f.path]).map((f) => f.path);
      const res = await api.decide(a.id, approve, files);
      toast(approve ? t(`Freed ${bytes(res?.freed_bytes ?? 0)}`, `${bytes(res?.freed_bytes ?? 0)} jagah khaali hui`) : t("OK, nothing was removed", "Theek hai, kuch nahi hataya"), approve ? "good" : "info");
      await load();
    } catch (e) {
      toast(String(e), "warn");
    } finally {
      busy = null;
    }
  }

  const KIND = $derived<Record<string, { icon: string; label: string }>>({
    daily: { icon: "bird", label: t("Daily report", "Daily report") }, weekly: { icon: "bird", label: t("Weekly report", "Weekly report") },
    alert: { icon: "alert", label: "Alert" }, cleanup: { icon: "broom", label: t("Cleanup", "Safai") },
    approval: { icon: "lock", label: t("Approval", "Ijazat") }, tip: { icon: "info", label: "Tip" },
  });
</script>

<header class="head"><div><h1>Messages</h1><p class="muted">{t("Reports, alerts and requests that need your approval. Everything stays on this PC.", "Sweeplark ki reports, alerts aur aapki ijazat wale kaam. Sab kuch isi PC par.")}</p></div></header>

{#each approvals as a (a.id)}
  <div class="card approval fade">
    <div class="row"><span class="badge"><Icon name="lock" size={18} /></span>
      <div class="col grow" style="gap:2px"><h3>{a.title}</h3><span class="muted small">{ago(a.at)} · {t("nothing is deleted without your approval", "aapki ijazat ke bina kuch delete nahi hoga")}{a.detail.ai_reviewed ? t(` · AI checked ${a.detail.ai_reviewed} files`, ` · AI ne ${a.detail.ai_reviewed} files jaanchi`) : ""}</span></div></div>
    <div class="list">
      {#each a.detail.cleaners as c}
        <div class="item"><Icon name="check" size={16} /><span>{c.name}</span>{#if c.risk !== "safe"}<span class="chip ask">{t("Asks first", "Poochega")}</span>{/if}
          {#if c.note}<span class="muted small">{c.note}</span>{/if}<span class="spacer"></span><span class="num">{bytes(c.bytes)}</span></div>
      {/each}
    </div>
    {#if a.detail.risky_files.length}
      <div class="risky">
        <p class="small"><Icon name="alert" size={14} /> <strong>{t("These files may be yours.", "Ye files aapki ho sakti hain.")}</strong> {t("Only ticked files are removed:", "Sirf tick ki hui hategi:")}</p>
        {#each a.detail.risky_files as f}
          <label class="frow"><input type="checkbox" bind:checked={ticks[f.path]} /><span class="chip">{f.kind}</span>
            <span class="path selectable" title={f.path}>{shortPath(f.path, 72)}</span><span class="spacer"></span><span class="num muted">{bytes(f.bytes)}</span></label>
          {#if f.review}<p class="review small">AI: {f.review}</p>{/if}
        {/each}
      </div>
    {/if}
    <div class="row actions">
      <button class="btn" onclick={() => decide(a, false)} disabled={busy === a.id}>{t("No, keep them", "Nahi, rehne do")}</button>
      <button class="btn primary" onclick={() => decide(a, true)} disabled={busy === a.id}>
        {#if busy === a.id}<div class="spin" style="border-top-color:#fff"></div>{/if}{t("Yes, clean", "Haan, saaf karo")}</button>
    </div>
  </div>
{/each}

<div class="col">
  {#each messages.filter((m) => m.kind !== "approval" || !approvals.some((a) => a.id === m.data.approval_id)) as m (m.id)}
    <div class="card msg {m.kind}" class:unread={!m.read}>
      <span class="badge"><Icon name={KIND[m.kind]?.icon ?? "info"} size={18} /></span>
      <div class="col grow" style="gap:6px">
        <div class="row"><strong>{m.title}</strong><span class="chip">{KIND[m.kind]?.label}</span><span class="spacer"></span><span class="muted small">{ago(m.at)}</span></div>
        {#if m.ai_body}<p class="ai selectable">{m.ai_body}</p>{/if}
        <pre class="selectable">{m.body}</pre>
      </div>
    </div>
  {:else}
    {#if !approvals.length}<div class="card empty"><Icon name="inbox" size={30} /><p>{t("No messages yet. Your first daily report arrives this evening.", "Abhi koi message nahi. Shaam ko pehli daily report aayegi.")}</p></div>{/if}
  {/each}
</div>

<style>
  .head { margin-bottom: 18px; }
  .grow { flex: 1; min-width: 0; }
  .badge { width: 36px; height: 36px; border-radius: 11px; display: grid; place-items: center; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .approval { border: 1px solid var(--amber); border-left-width: 4px; margin-bottom: 14px; display: flex; flex-direction: column; gap: 10px; }
  .approval .badge { background: var(--amber); color: #3a2500; }
  .risky { display: flex; flex-direction: column; gap: 6px; padding: 12px; border-radius: 12px; background: var(--surface-2); border: 1px solid var(--line); }
  .frow { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
  .path { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .review { margin: -2px 0 2px 25px; color: var(--teal-text); }
  .actions { justify-content: flex-end; }
  .msg { display: flex; gap: 12px; padding: 16px; }
  .msg.unread { border-left: 3px solid var(--teal); }
  .msg.alert .badge { background: var(--amber-soft); color: var(--amber-text); }
  .ai { padding: 10px 12px; border-radius: 10px; background: var(--surface-2); border-left: 3px solid var(--teal); font-size: 14.5px; }
  pre { font-family: inherit; white-space: pre-wrap; margin: 0; color: var(--ink-2); font-size: 13px; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 8px; }
</style>
