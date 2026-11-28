<script lang="ts">
  // Dialog. Use `bind:open`, or pass `open={...}` together with `onclose` that clears the
  // state behind it (otherwise a dialog closed with ✕ could not open again).
  import type { Snippet } from "svelte";
  import { t } from "../lib/state.svelte";
  let { open = $bindable(false), title, children, footer, wide = false, onclose }:
    { open?: boolean; title: string; children: Snippet; footer?: Snippet; wide?: boolean; onclose?: () => void } = $props();

  let dialog = $state<HTMLDivElement | null>(null);
  function close() {
    open = false;
    onclose?.();
  }
  // Focus moves into the dialog, Escape closes it from anywhere, focus goes back after.
  $effect(() => {
    if (!open) return;
    const before = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      // In order of preference: a text field, the main button, any footer button.
      const pick = ["input:not([type=checkbox]), textarea", "footer .btn.primary", "footer .btn"]
        .map((sel) => dialog?.querySelector<HTMLElement>(sel))
        .find(Boolean);
      (pick ?? dialog)?.focus();
    });
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); close(); }
      if (e.key === "Tab" && dialog) {
        const items = [...dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), textarea, select, [href]")];
        if (!items.length) return;
        const [a, z] = [items[0], items[items.length - 1]];
        if (e.shiftKey && document.activeElement === a) { e.preventDefault(); z.focus(); }
        else if (!e.shiftKey && document.activeElement === z) { e.preventDefault(); a.focus(); }
      }
    };
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("keydown", key); before?.focus?.(); };
  });
</script>

{#if open}
  <div class="backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
    <div class="modal fade" class:wide role="dialog" aria-modal="true" aria-label={title} tabindex="-1" bind:this={dialog}>
      <header><h2>{title}</h2><button class="btn ghost small" onclick={close} aria-label={t("Close", "Band karein")}>✕</button></header>
      <div class="body">{@render children()}</div>
      {#if footer}<footer>{@render footer()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(11, 27, 51, .38); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 50; padding: 24px; }
  .modal { width: min(520px, 100%); max-height: 86vh; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--line); border-radius: 20px; box-shadow: var(--shadow-lg); }
  .modal:focus { outline: none; }
  .modal.wide { width: min(760px, 100%); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 18px 20px 6px; }
  .body { padding: 8px 20px 16px; overflow: auto; }
  footer { display: flex; justify-content: flex-end; gap: 10px; padding: 14px 20px; border-top: 1px solid var(--line); }
</style>
