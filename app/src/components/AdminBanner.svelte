<script lang="ts">
  // Shown where some items need administrator rights. One click restarts Sweeplark as
  // administrator (Windows asks first) and comes back to this same page; settings, history
  // and results of the AI model are shared, so nothing is lost.
  import Icon from "./Icon.svelte";
  import { api } from "../lib/api";
  import { t, toast, ui } from "../lib/state.svelte";

  // `count` undefined = show whenever not admin; a number = show only when above 0.
  let { text = "", count = undefined }: { text?: string; count?: number } = $props();

  let busy = $state(false);
  async function restart() {
    if (busy) return;
    busy = true;
    try { await api.relaunchAdmin(ui.page); } catch (e) { toast(String(e), "warn"); } finally { busy = false; }
  }
</script>

{#if ui.info && !ui.info.is_admin && (count === undefined || count > 0)}
  <div class="admin fade" role="note">
    <span class="ic"><Icon name="lock" size={16} /></span>
    <div class="col grow" style="gap:1px">
      <strong>{count ? t(`${count} items need administrator rights`, `${count} cheezon ke liye admin chahiye`) : t("Some items need administrator rights", "Kuch cheezon ke liye admin chahiye")}</strong>
      <span class="muted small">{text || t("Restart Sweeplark as administrator to do them. You come back to this page; settings and history are kept (a scan may need to run again).", "Unhe karne ke liye Sweeplark ko admin mode mein kholein. Aap isi page par wapas aayenge; settings aur history rahegi (scan dobara karna pad sakta hai).")}</span>
    </div>
    <button class="btn small primary" disabled={busy} onclick={restart}><Icon name="lock" size={14} />{t("Restart as admin", "Admin mode mein kholo")}</button>
  </div>
{/if}

<style>
  .admin { display: flex; align-items: center; gap: 12px; padding: 12px 14px; border-radius: 14px; border: 1px solid var(--amber); background: var(--amber-soft); margin-bottom: 14px; flex-wrap: wrap; }
  .ic { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 9px; background: var(--surface); color: var(--amber-text); flex: none; }
  .grow { flex: 1; min-width: 220px; }
  strong { color: var(--amber-text); }
</style>
