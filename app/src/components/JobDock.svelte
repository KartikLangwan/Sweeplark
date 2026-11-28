<script lang="ts">
  // Small list of running jobs in the sidebar: visible from every page, click to open.
  import { jobs, openJob, percent, type Job } from "../lib/jobs.svelte";

  const list = $derived(Object.values(jobs).filter((j): j is Job => !!j?.running));
</script>

{#if list.length}
  <div class="dock">
    {#each list as j (j.id)}
      {@const p = percent(j)}
      <button class="item" onclick={() => openJob(j)} title={j.label}>
        <span class="row"><span class="dot"></span><span class="lbl">{j.label}</span><span class="spacer"></span><span class="num">{p != null ? `${Math.floor(p)}%` : "…"}</span></span>
        <span class="bar" class:ind={p == null}><i style="width:{p ?? 35}%"></i></span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .dock { display: flex; flex-direction: column; gap: 6px; }
  .item { display: flex; flex-direction: column; gap: 5px; padding: 8px 10px; border-radius: 10px; border: 1px solid var(--line); background: var(--surface); cursor: pointer; text-align: left; color: var(--ink); font-size: 12px; }
  .item:hover { border-color: var(--teal); }
  .lbl { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 560; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--teal); flex: none; animation: pulse 1.2s ease-in-out infinite; }
  .bar { height: 4px; border-radius: 999px; background: var(--surface-3); overflow: hidden; position: relative; }
  .bar i { display: block; height: 100%; background: var(--teal); border-radius: 999px; transition: width .4s; }
  .bar.ind i { position: absolute; animation: slide 1.3s ease-in-out infinite; }
  @keyframes slide { 0% { left: -35%; } 100% { left: 100%; } }
  @keyframes pulse { 50% { opacity: .35; } }
  @media (prefers-reduced-motion: reduce) { .dot, .bar.ind i { animation: none; } }
</style>
