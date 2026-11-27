<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Ring from "../components/Ring.svelte";
  import Modal from "../components/Modal.svelte";
  import FindingCard from "../components/FindingCard.svelte";
  import { api } from "../lib/api";
  import { bytes, pct, uptime } from "../lib/format";
  import { go, toast, ui , t } from "../lib/state.svelte";
  import type { Health, Message, OptimizePlan, OptimizeReport } from "../lib/types";

  let health = $state<Health | null>(null);
  let plan = $state<OptimizePlan | null>(null);
  let report = $state<OptimizeReport | null>(null);
  let phase = $state<"idle" | "scanning" | "planned" | "fixing" | "done">("idle");
  let latest = $state<Message | null>(null);
  let showPlan = $state(false);

  onMount(() => {
    api.health(true).then((h) => (health = h));
    api.inbox().then((b) => (latest = b.messages.find((m) => m.kind === "daily" || m.kind === "weekly") ?? null));
    const t = setInterval(() => api.health(false).then((h) => { if (health) health = { ...h, security: health.security, battery: health.battery }; }), 8000);
    return () => clearInterval(t);
  });

  const ram = $derived(health ? (health.memory_used / health.memory_total) * 100 : 0);
  const sys = $derived(health?.disks.find((d) => d.is_system));
  const diskUsed = $derived(sys ? (1 - sys.free_bytes / sys.total_bytes) * 100 : 0);
  const score = $derived.by(() => {
    if (!health) return 0;
    let s = 100;
    s -= Math.max(0, ram - 70) * 0.9;
    s -= Math.max(0, diskUsed - 80) * 1.2;
    s -= Math.max(0, health.cpu_percent - 70) * 0.4;
    if (health.uptime_secs > 7 * 86400) s -= 6;
    if (health.security?.defender_realtime_enabled === false) s -= 25;
    if (plan) s -= Math.min(10, plan.auto_fix_bytes / 1024 ** 3 * 2);
    return Math.max(5, Math.round(s));
  });
  const mood = $derived(score >= 85 ? t("Looking great", "Bahut badhiya") : score >= 65 ? t("Doing okay, needs a little care", "Theek hai, thoda dhyan chahiye") : t("Needs attention", "Dhyan chahiye"));

  async function optimize() {
    phase = "scanning";
    report = null;
    try {
      plan = await api.optimizePlan();
      health = plan.health;
      phase = "planned";
    } catch (e) {
      toast(String(e), "warn");
      phase = "idle";
    }
  }

  async function fix() {
    phase = "fixing";
    try {
      report = await api.optimizeApply();
      phase = "done";
      toast(t(`Freed ${bytes(report.clean.freed_bytes)}`, `${bytes(report.clean.freed_bytes)} jagah khaali hui`));
      api.health(false).then((h) => (health = { ...h, security: health?.security ?? null, battery: health?.battery ?? null }));
    } catch (e) {
      toast(String(e), "warn");
      phase = "planned";
    }
  }
</script>

<header class="head">
  <div>
    <h1>{t("Hello", "Namaste")}</h1>
    <p class="muted">{health ? `${health.os} · ${health.cpu_brand}` : t("Reading your PC…", "PC ki jaankari aa rahi hai…")}</p>
  </div>
</header>

<section class="hero card">
  <Ring value={score} size={150} stroke={13} label={health ? String(score) : "–"} sub={t("health score", "sehat score")} tone={score < 50 ? "red" : score < 70 ? "amber" : "teal"} />
  <div class="hero-text">
    {#if phase === "idle"}
      <h2>{health ? mood : t("One moment…", "Ek pal…")}</h2>
      <p class="muted">{t("One click checks junk, startup, storage, health and security. You see the results first; only safe items are fixed.", "Ek click mein junk, startup, storage, sehat aur security sab check hoga. Pehle result dikhega, phir sirf safe cheezein theek hongi.")}</p>
      <div class="row"><button class="btn primary big" onclick={optimize}><Icon name="sparkle" />One-Click Optimize</button></div>
    {:else if phase === "scanning"}
      <h2>{t("Scanning…", "Scan ho raha hai…")}</h2>
      <p class="muted">{t("Checking temp files, browser caches, startup apps, RAM, disks and security. Nothing is being changed.", "Temp files, browser cache, startup apps, RAM, disk aur security dekh raha hoon. Kuch bhi badla nahi ja raha.")}</p>
      <div class="row"><div class="spin"></div><span class="muted">{t("Just a moment…", "Thoda sa intezaar…")}</span></div>
    {:else if phase === "planned" && plan}
      <h2>{t(`${bytes(plan.auto_fix_bytes)} ready to clean safely`, `${bytes(plan.auto_fix_bytes)} safe safai taiyaar`)}</h2>
      <p class="muted">
        {t(`Only caches and temp files from ${plan.auto_fix_ids.length} cleaners. Documents and photos are never touched.`, `${plan.auto_fix_ids.length} cleaners mein sirf cache aur temp files. Documents ya photos jaisi files nahi chhui jayengi.`)}
        {#if plan.startup_suggestions.length}{t(`You decide about ${plan.startup_suggestions.length} startup apps.`, `${plan.startup_suggestions.length} startup apps ka faisla aap karenge.`)}{/if}
      </p>
      <div class="row">
        <button class="btn primary big" onclick={fix} disabled={!plan.auto_fix_ids.length}><Icon name="check" />{t("Fix safe items", "Safe cheezein theek karo")}</button>
        <button class="btn" onclick={() => (showPlan = true)}>{t("What will change?", "Kya-kya hoga?")}</button>
      </div>
    {:else if phase === "fixing"}
      <h2>{t("Cleaning…", "Safai ho rahi hai…")}</h2>
      <div class="row"><div class="spin"></div><span class="muted">{t("Every change is being recorded.", "Har badlav hisaab mein likha ja raha hai.")}</span></div>
    {:else if phase === "done" && report}
      <h2>{t(`Done! ${bytes(report.clean.freed_bytes)} freed`, `Ho gaya! ${bytes(report.clean.freed_bytes)} khaali`)}</h2>
      <p class="muted">
        {t(`Removed ${report.clean.deleted_files.toLocaleString()} junk files.`, `${report.clean.deleted_files.toLocaleString()} bekaar files hatayi.`)}
        {#if report.system_free_before != null && report.system_free_after != null}
          {t("System drive free:", "System drive khaali:")} {bytes(report.system_free_before)} → <strong>{bytes(report.system_free_after)}</strong>
        {/if}
      </p>
      <div class="row">
        <button class="btn" onclick={() => go("history")}><Icon name="history" />{t("View history", "Report dekhein")}</button>
        <button class="btn ghost" onclick={() => (phase = "idle")}>{t("OK", "Theek hai")}</button>
      </div>
    {/if}
  </div>
</section>

<section class="stats">
  <div class="card stat">
    <div class="row"><span class="ic"><Icon name="ram" /></span><span class="muted">RAM</span></div>
    <strong class="num">{health ? pct(ram) : "–"}</strong>
    <div class="bar" class:amber={ram > 80} class:red={ram > 92}><span style="width:{ram}%"></span></div>
    <span class="muted small">{health ? `${bytes(health.memory_used)} / ${bytes(health.memory_total)}` : ""}</span>
  </div>
  <div class="card stat">
    <div class="row"><span class="ic"><Icon name="cpu" /></span><span class="muted">CPU</span></div>
    <strong class="num">{health ? pct(health.cpu_percent) : "–"}</strong>
    <div class="bar" class:amber={(health?.cpu_percent ?? 0) > 80}><span style="width:{health?.cpu_percent ?? 0}%"></span></div>
    <span class="muted small">{health ? `${health.cpu_cores} threads · ${health.process_count} processes` : ""}</span>
  </div>
  <div class="card stat">
    <div class="row"><span class="ic"><Icon name="disk" /></span><span class="muted">{sys ? sys.mount.replace("\\", "") : "Disk"} {t("free", "khaali")}</span></div>
    <strong class="num">{sys ? bytes(sys.free_bytes) : "–"}</strong>
    <div class="bar" class:amber={diskUsed > 85} class:red={diskUsed > 92}><span style="width:{diskUsed}%"></span></div>
    <span class="muted small">{sys ? t(`${sys.kind} · of ${bytes(sys.total_bytes)}`, `${sys.kind} · ${bytes(sys.total_bytes)} mein se`) : ""}</span>
  </div>
  <div class="card stat">
    <div class="row"><span class="ic"><Icon name="shield" /></span><span class="muted">{t("Security", "Suraksha")}</span></div>
    {#if health?.security}
      {@const ok = health.security.defender_realtime_enabled !== false && health.security.firewall_profiles.every(([, on]) => on)}
      <strong class:warn={!ok}>{ok ? t("Protected", "Surakshit") : t("Check this", "Dhyan dein")}</strong>
      <span class="muted small">Defender {health.security.defender_realtime_enabled === false ? t("off", "band") : t("on", "chalu")} · Firewall {health.security.firewall_profiles.every(([, on]) => on) ? t("on", "chalu") : t("off", "band")}</span>
    {:else}
      <strong>–</strong><span class="muted small">{health ? t("Not available", "Jaankari nahi mili") : ""}</span>
    {/if}
    <span class="muted small">Uptime {health ? uptime(health.uptime_secs) : "–"}</span>
  </div>
</section>

<section class="two">
  <div class="card">
    <div class="card-title"><h3>{t("Worth a look", "Kya dhyan dena hai")}</h3><span class="spacer"></span>
      <button class="btn ghost small" onclick={() => go("assistant")}><Icon name="chat" size={16} />{t("Ask the assistant", "AI se poochein")}</button></div>
    {#if plan}
      <div class="col">{#each plan.findings.slice(0, 5) as f}<FindingCard finding={f} />{/each}</div>
    {:else}
      <div class="empty"><Icon name="sparkle" size={28} /><p>{t("Run One-Click Optimize to see everything here.", "One-Click Optimize chalayein; saari baatein yahan dikhengi.")}</p></div>
    {/if}
  </div>
  <div class="card summary">
    <div class="card-title"><span class="ic"><Icon name="bird" /></span><h3>{latest?.title ?? t("Today's report", "Aaj ki report")}</h3></div>
    {#if latest}
      {#if latest.ai_body}<p class="ai selectable">{latest.ai_body}</p>{/if}
      <pre class="selectable">{latest.body}</pre>
      <button class="btn small" onclick={() => go("inbox")}>{t("All messages", "Saare messages")}</button>
    {:else}
      <p class="muted">{t(`Every evening at ${ui.settings?.agent.daily_summary ?? "21:00"} Sweeplark sends a short report: space freed, when RAM was full, and the heaviest app.`, `Har shaam ${ui.settings?.agent.daily_summary ?? "21:00"} baje Sweeplark aapko din ki chhoti si report dega: kitni jagah bachi, RAM kab bhari, kaun si app sabse bhari rahi.`)}</p>
    {/if}
  </div>
</section>

<Modal bind:open={showPlan} title={t("What One-Click will do", "One-Click kya karega")} wide>
  {#if plan}
    <p class="muted" style="margin-bottom:12px">{t("Only safe cleaners run. Everything else is left for you to decide.", "Sirf safe cleaners chalenge. Baaki sab aapke faisle ke liye chhoda jayega.")}</p>
    <div class="list">
      {#each plan.scan.cleaners.filter((c) => plan!.auto_fix_ids.includes(c.id)) as c}
        <div class="item"><Icon name="check" size={16} /><span>{c.name}</span><span class="spacer"></span>
          {#if c.flagged.length}<span class="chip ask">{t(`${c.flagged.length} files kept`, `${c.flagged.length} files chhodi jayengi`)}</span>{/if}
          <strong class="num">{bytes(c.bytes - c.flagged_bytes)}</strong></div>
      {/each}
    </div>
  {/if}
  {#snippet footer()}<button class="btn primary" onclick={() => { showPlan = false; fix(); }}>{t("Fix now", "Theek karo")}</button>{/snippet}
</Modal>

<style>
  .head { display: flex; align-items: flex-end; justify-content: space-between; margin-bottom: 18px; }
  .hero { display: flex; align-items: center; gap: 32px; padding: 28px 32px; margin-bottom: 18px; }
  .hero-text { display: flex; flex-direction: column; gap: 12px; max-width: 620px; }
  .hero-text h2 { font-size: 24px; }
  .stats { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; margin-bottom: 16px; }
  .stat { display: flex; flex-direction: column; gap: 8px; padding: 16px; }
  .stat strong { font-size: 24px; font-weight: 650; }
  .stat strong.warn { color: var(--amber-text); }
  .ic { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 8px; background: var(--teal-soft); color: var(--teal-text); }
  .two { display: grid; grid-template-columns: 1.4fr 1fr; gap: 14px; }
  .summary { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
  .summary .ai { font-size: 14.5px; line-height: 1.55; padding: 12px 14px; border-radius: 10px; background: var(--surface-2); border-left: 3px solid var(--teal); color: var(--ink); }
  pre { font-family: inherit; white-space: pre-wrap; margin: 0; color: var(--ink-2); font-size: 13px; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 8px; }
  @media (max-width: 1050px) { .stats { grid-template-columns: repeat(2, 1fr); } .two { grid-template-columns: 1fr; } }
</style>
