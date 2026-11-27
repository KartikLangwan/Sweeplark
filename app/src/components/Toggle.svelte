<script lang="ts">
  let { checked = $bindable(false), disabled = false, label = "", onchange }:
    { checked?: boolean; disabled?: boolean; label?: string; onchange?: (v: boolean) => void } = $props();
</script>

<button type="button" class="toggle" class:on={checked} {disabled} role="switch" aria-checked={checked} aria-label={label}
  onclick={() => { checked = !checked; onchange?.(checked); }}>
  <span></span>
</button>

<style>
  .toggle { width: 42px; height: 24px; border-radius: 999px; border: 0; padding: 0; background: var(--line-strong); position: relative; cursor: pointer; flex: none; transition: background .2s; }
  .toggle span { position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; box-shadow: 0 1px 3px rgba(0,0,0,.25); transition: transform .2s; }
  .toggle.on { background: var(--teal); }
  .toggle.on span { transform: translateX(18px); }
  .toggle:disabled { opacity: .45; cursor: default; }
  .toggle:focus-visible { outline: 2px solid var(--teal); outline-offset: 2px; }
</style>
