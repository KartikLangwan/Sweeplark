<script lang="ts">
  import Icon from "./Icon.svelte";
  import { actionLabel as label, runAction as run } from "../lib/actions";
  import type { Finding } from "../lib/types";

  let { finding, compact = false }: { finding: Finding; compact?: boolean } = $props();

  const icon = $derived(finding.severity === "critical" || finding.severity === "warning" ? "alert" : finding.severity === "good" ? "check" : "info");

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
