<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api } from "../lib/api";
  import { go, toast , t } from "../lib/state.svelte";
  import type { Finding, SuggestedAction } from "../lib/types";

  let { finding, compact = false }: { finding: Finding; compact?: boolean } = $props();

  const icon = $derived(finding.severity === "critical" || finding.severity === "warning" ? "alert" : finding.severity === "good" ? "check" : "info");

  function label(a: SuggestedAction): string {
    switch (a.kind) {
      case "clean": return t("Review cleanup", "Safai dekhein");
      case "review_startup": return t("Review startup", "Startup dekhein");
      case "restart": return t("How to restart", "Restart ki yaad");
      case "open_windows_security": return t("Open Windows Security", "Windows Security kholein");
      case "close_apps": return t("See in Health", "Sehat mein dekhein");
      case "backup": return t("How to back up", "Backup kaise lein");
    }
  }

  async function run(a: SuggestedAction) {
    switch (a.kind) {
      case "clean": return go("clean");
      case "review_startup": return go("startup");
      case "close_apps": return go("health");
      case "open_windows_security": return api.openSecurity();
      case "restart": return toast(t("Save your work, then Start → Power → Restart.", "Kaam save karke Start → Power → Restart karein."), "info");
      case "backup": return toast(t("Copy important files to a USB drive or cloud storage.", "Zaruri files ko pen drive ya cloud par copy karein."), "info");
    }
  }
</script>

<div class="finding {finding.severity}" class:compact>
  <span class="dot"><Icon name={icon} size={16} /></span>
  <div class="text">
    <strong>{finding.title}</strong>
    {#if !compact}<p class="muted selectable">{finding.detail}</p>{/if}
  </div>
  {#if finding.action}
    <button class="btn small" onclick={() => run(finding.action!)}>{label(finding.action)}<Icon name="chevron" size={14} /></button>
  {/if}
</div>

<style>
  .finding { display: flex; gap: 12px; align-items: flex-start; padding: 12px 4px; border-bottom: 1px solid var(--line); }
  .finding:last-child { border-bottom: 0; }
  .finding.compact { align-items: center; padding: 8px 12px; border: 1px solid var(--line); border-radius: 10px; }
  .dot { flex: none; display: grid; place-items: center; width: 28px; height: 28px; border-radius: 50%; background: var(--teal-soft); color: var(--teal-text); }
  .warning .dot { background: var(--amber-soft); color: var(--amber-text); }
  .critical .dot { background: var(--red-soft); color: var(--red-text); }
  .good .dot { background: var(--good-soft); color: var(--good); }
  .text { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .text p { font-size: 13px; }
  .btn { flex: none; align-self: center; }
</style>
