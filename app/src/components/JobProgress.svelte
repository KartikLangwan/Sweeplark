<script lang="ts">
  // Big progress card for a running job: bar, percentage (or a moving bar when the total
  // is unknown), elapsed time, what is being looked at, and a rotating tip.
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { jobs, percent, type JobId } from "../lib/jobs.svelte";
  import { tipAt } from "../lib/tips";
  import { t, ui } from "../lib/state.svelte";

  let { id, title = "", hint = "" }: { id: JobId; title?: string; hint?: string } = $props();

  let now = $state(Date.now());
  let tip = $state(Math.floor(Math.random() * 40));
  onMount(() => {
    const a = setInterval(() => (now = Date.now()), 1000);
    const b = setInterval(() => (tip += 1), 6000);
    return () => { clearInterval(a); clearInterval(b); };
  });

  const job = $derived(jobs[id]);
  const p = $derived(job ? percent(job) : null);
  const secs = $derived(job ? Math.max(0, Math.round((now - job.started) / 1000)) : 0);
  const elapsed = $derived(secs < 60 ? `${secs}s` : `${Math.floor(secs / 60)}m ${secs % 60}s`);
  const eta = $derived.by(() => {
    if (!job || p == null || p < 3 || secs < 3) return "";
    const left = Math.round((secs * (100 - p)) / p);
    return left < 60 ? t(`~${left}s left`, `~${left}s baaki`) : t(`~${Math.ceil(left / 60)} min left`, `~${Math.ceil(left / 60)} min baaki`);
  });
</script>

{#if job}
  <div class="card job fade" role="status" aria-live="polite">
    <div class="row">
      <div class="spin"></div>
      <strong>{title || job.label}</strong>
      <span class="spacer"></span>
      <span class="num big">{p != null ? `${Math.floor(p)}%` : job.done ? job.done.toLocaleString() : ""}</span>
    </div>
    <div class="bar" class:indeterminate={p == null}><span style="width:{p ?? 30}%"></span></div>
    <div class="row small muted">
      <span class="ell">{job.text ? job.text : hint}</span>
      <span class="spacer"></span>
      <span>{elapsed}{eta ? ` · ${eta}` : ""}</span>
    </div>
    <p class="tip small"><Icon name="sparkle" size={13} />{tipAt(tip, ui.settings?.language === "hinglish")}</p>
    <p class="muted small">{t("You can open other pages; this keeps running.", "Aap doosre page khol sakte hain; yeh chalta rahega.")}</p>
  </div>
{/if}

<style>
  .job { display: flex; flex-direction: column; gap: 10px; margin-bottom: 14px; }
  .big { font-size: 20px; font-weight: 650; color: var(--teal-text); }
  .bar { height: 10px; border-radius: 999px; background: var(--surface-3); overflow: hidden; position: relative; }
  .bar span { display: block; height: 100%; border-radius: 999px; background: linear-gradient(90deg, var(--teal), var(--mint)); transition: width .4s ease; }
  .bar.indeterminate span { position: absolute; animation: slide 1.4s ease-in-out infinite; }
  @keyframes slide { 0% { left: -30%; } 100% { left: 100%; } }
  .ell { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 70%; }
  .tip { display: flex; gap: 7px; align-items: flex-start; padding: 9px 11px; border-radius: 10px; background: var(--teal-soft); color: var(--teal-text); }
  @media (prefers-reduced-motion: reduce) { .bar.indeterminate span { animation: none; left: 0; width: 100% !important; opacity: .5; } }
</style>
