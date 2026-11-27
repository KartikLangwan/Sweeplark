<script lang="ts">
  // Circular gauge. value 0-100.
  let { value, size = 120, stroke = 11, label = "", sub = "", tone = "teal" }:
    { value: number; size?: number; stroke?: number; label?: string; sub?: string; tone?: "teal" | "amber" | "red" } = $props();
  const r = $derived((size - stroke) / 2);
  const c = $derived(2 * Math.PI * r);
  const dash = $derived(c * Math.min(1, Math.max(0, value / 100)));
  const color = $derived(tone === "red" ? "var(--red)" : tone === "amber" ? "var(--amber)" : "url(#ringgrad)");
</script>

<div class="ring" style="width:{size}px;height:{size}px">
  <svg width={size} height={size} viewBox="0 0 {size} {size}" role="img" aria-label="{label} {Math.round(value)}">
    <defs>
      <linearGradient id="ringgrad" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#7DE8C8" /><stop offset="1" stop-color="#0FB5AE" />
      </linearGradient>
    </defs>
    <circle cx={size / 2} cy={size / 2} {r} fill="none" stroke="var(--surface-3)" stroke-width={stroke} />
    <circle cx={size / 2} cy={size / 2} {r} fill="none" stroke={color} stroke-width={stroke} stroke-linecap="round"
      stroke-dasharray="{dash} {c}" transform="rotate(-90 {size / 2} {size / 2})" style="transition: stroke-dasharray .6s ease" />
  </svg>
  <div class="center">
    <strong class="num" style="font-size:{size * 0.24}px">{label}</strong>
    {#if sub}<span>{sub}</span>{/if}
  </div>
</div>

<style>
  .ring { position: relative; flex: none; }
  .center { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; }
  strong { font-weight: 650; line-height: 1; }
  span { font-size: 11.5px; color: var(--slate); margin-top: 4px; }
</style>
