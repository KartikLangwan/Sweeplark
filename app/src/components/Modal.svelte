<script lang="ts">
  import type { Snippet } from "svelte";
  let { open = $bindable(false), title, children, footer, wide = false }:
    { open?: boolean; title: string; children: Snippet; footer?: Snippet; wide?: boolean } = $props();
</script>

{#if open}
  <div class="backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) open = false; }}
    onkeydown={(e) => { if (e.key === "Escape") open = false; }}>
    <div class="modal fade" class:wide role="dialog" aria-modal="true" aria-label={title} tabindex="-1">
      <header><h2>{title}</h2><button class="btn ghost small" onclick={() => (open = false)} aria-label="Close">✕</button></header>
      <div class="body">{@render children()}</div>
      {#if footer}<footer>{@render footer()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(11, 27, 51, .38); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 50; padding: 24px; }
  .modal { width: min(520px, 100%); max-height: 86vh; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--line); border-radius: 20px; box-shadow: var(--shadow-lg); }
  .modal.wide { width: min(760px, 100%); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 18px 20px 6px; }
  .body { padding: 8px 20px 16px; overflow: auto; }
  footer { display: flex; justify-content: flex-end; gap: 10px; padding: 14px 20px; border-top: 1px solid var(--line); }
</style>
