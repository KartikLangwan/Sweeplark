<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { api, on } from "../lib/api";
  import { bytes } from "../lib/format";
  import { applyTheme, saveSettings, t, toast, ui } from "../lib/state.svelte";
  import type { AiStatus, InstallProgress, Settings, Tier } from "../lib/types";

  let s = $state<Settings | null>(null);
  let ai = $state<AiStatus | null>(null);
  let progress = $state<InstallProgress | null>(null);
  let newPath = $state("");
  const DAYS = $derived(ui.settings?.language === "hinglish"
    ? ["Somvar", "Mangalvar", "Budhvar", "Guruvar", "Shukravar", "Shanivar", "Ravivar"]
    : ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]);

  onMount(() => {
    api.settings().then((x) => (s = structuredClone(x)));
    loadAi();
    const u = on<InstallProgress>("ai-progress", (p) => {
      progress = p;
      if (p.stage === "done") { toast(t("AI model is ready", "AI model taiyaar hai")); loadAi(); api.settings().then((x) => (s = structuredClone(x))); }
      if (p.stage === "error") toast(p.error ?? t("Download failed", "Download nahi hua"), "warn");
    });
    return () => { u.then((f) => f()); };
  });

  async function loadAi() {
    ai = await api.aiStatus();
    progress = ai.progress;
  }

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  function save() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      if (!s) return;
      try { await saveSettings($state.snapshot(s) as Settings); toast(t("Settings saved", "Settings save ho gayi")); } catch (e) { toast(String(e), "warn"); }
    }, 350);
  }

  async function install(tier: Tier) {
    try { await api.aiInstall(tier); progress = { stage: "model", tier, done: 0, total: 1, error: null }; }
    catch (e) { toast(String(e), "warn"); }
  }
  async function remove(tier: Tier) {
    await api.aiRemove(tier);
    toast(t("Model removed", "Model hata diya"));
    await loadAi();
    s = structuredClone(await api.settings());
  }
  function useModel(tier: Tier) {
    if (!s) return;
    s.ai.tier = tier; s.ai.enabled = true; save(); loadAi();
  }
  const downloading = $derived(progress && (progress.stage === "runtime" || progress.stage === "model"));
  const TIER_LABEL: Record<Tier, string> = { lite: "Lite", balanced: "Balanced", power: "High" };
</script>

<header class="head"><div><h1>Settings</h1><p class="muted">{t("You are in control. Changes save instantly.", "Har cheez aapke control mein. Badlav turant save hote hain.")}</p></div></header>

{#if s}
  <section class="card">
    <div class="card-title"><Icon name="sparkle" /><h2>{t("General", "General")}</h2></div>
    <div class="setting"><div><strong>{t("Language", "Bhasha (Language)")}</strong><p class="muted small">{t("Used across the app, notifications, reports and the assistant.", "Poore app, notifications, reports aur assistant mein.")}</p></div>
      <div class="seg"><button class:on={s.language === "en"} onclick={() => { s!.language = "en"; save(); }}>English</button>
        <button class:on={s.language === "hinglish"} onclick={() => { s!.language = "hinglish"; save(); }}>Hinglish</button></div></div>
    <div class="setting"><div><strong>{t("Theme", "Theme")}</strong><p class="muted small">{t("Follow Windows, or pick light or dark.", "Windows ke hisaab se, ya light/dark chunein.")}</p></div>
      <div class="seg">{#each [["system", t("System", "System")], ["light", t("Light", "Light")], ["dark", t("Dark", "Dark")]] as [id, label]}<button class:on={ui.theme === id} onclick={() => applyTheme(id as "system" | "light" | "dark")}>{label}</button>{/each}</div></div>
  </section>
  <section class="card">
    <div class="card-title"><Icon name="shield" /><h2>{t("Trust and approvals", "Bharosa aur ijazat")}</h2></div>
    <div class="setting"><div><strong>{t("Trust level", "Bharose ka level")}</strong><p class="muted small">{t("Safe: only safe items are shown. Ask: items that ask first too. Expert: everything (always asks).", "Safe: sirf safe cheezein dikhengi. Ask: poochne wali bhi. Expert: sab kuch (hamesha pooch kar).")}</p></div>
      <div class="seg">{#each ["safe", "ask", "expert"] as t}<button class:on={s.trust_level === t} onclick={() => { s!.trust_level = t as Settings["trust_level"]; save(); }}>{t === "safe" ? "Safe" : t === "ask" ? "Ask" : "Expert"}</button>{/each}</div></div>
    <div class="setting"><div><strong>{t("Approval before auto-clean", "Auto safai se pehle ijazat")}</strong><p class="muted small">{t("Always ask (default): nothing is removed without your yes. Risky only: cache and temp are cleaned automatically, the rest asks with a notification. Files that look like documents or photos always ask.", "Har baar poocho (default): bina aapki haan ke kuch nahi hatega. Sirf risky: cache/temp khud saaf, baaki ke liye notification. Documents/photos jaisi files par hamesha poochega.")}</p></div>
      <div class="seg"><button class:on={s.agent.approval === "ask_always"} onclick={() => { s!.agent.approval = "ask_always"; save(); }}>{t("Always ask", "Har baar poocho")}</button>
        <button class:on={s.agent.approval === "risky_only"} onclick={() => { s!.agent.approval = "risky_only"; save(); }}>{t("Risky only", "Sirf risky par")}</button></div></div>
    <div class="setting"><div><strong>{t("Browser history and cookies", "Browser history aur cookies")}</strong><p class="muted small">{t("Privacy cleaners appear only when this is on. Removing cookies signs you out of websites.", "Chalu karne par hi privacy cleaners dikhenge. Cookies hatane se websites se logout ho jayenge.")}</p></div>
      <Toggle bind:checked={s.privacy_consent} onchange={save} label="Privacy cleaners" /></div>
    <div class="setting"><div><strong>Recycle Bin</strong><p class="muted small">{t("Only items older than this are removed.", "Itne din se purani cheezein hi hatengi.")}</p></div>
      <div class="row"><input type="number" min="0" max="365" bind:value={s.recycle_bin_min_age_days} onchange={save} style="width:80px" /><span class="muted">{t("days", "din")}</span></div></div>
    <div class="setting col-set"><div><strong>{t("Never touch (whitelist)", "Kabhi na chhuein (whitelist)")}</strong><p class="muted small">{t("The cleaner never touches these folders or files.", "Ye folders/files cleaner kabhi nahi chhuega.")}</p></div>
      <div class="col" style="width:100%">
        {#each s.whitelist as w, i}<div class="row"><code class="selectable">{w}</code><span class="spacer"></span><button class="btn ghost small" onclick={() => { s!.whitelist.splice(i, 1); save(); }}>{t("Remove", "Hatao")}</button></div>{/each}
        <div class="row"><input type="text" bind:value={newPath} placeholder="C:\Users\you\AppData\Local\Temp\MyProject" style="flex:1" />
          <button class="btn small" onclick={() => { if (newPath.trim()) { s!.whitelist.push(newPath.trim()); newPath = ""; save(); } }}>{t("Add", "Jodo")}</button></div>
      </div></div>
  </section>

  <section class="card">
    <div class="card-title"><Icon name="restart" /><h2>{t("Automation", "Automatic kaam")}</h2></div>
    <div class="setting"><div><strong>{t("Start with Windows", "Windows ke saath chalu ho")}</strong><p class="muted small">{t("Runs quietly in the tray. The background monitor uses no AI and almost no CPU.", "Tray mein chupchaap chalega. Background monitor bina AI ke, lagbhag 0% CPU.")}</p></div>
      <Toggle bind:checked={s.agent.start_with_windows} onchange={save} label="Start with Windows" /></div>
    <div class="setting"><div><strong>{t("Background monitor", "Background monitor")}</strong><p class="muted small">{t(`Notes RAM, CPU and disk every ${s.agent.sample_minutes} minutes.`, `Har ${s.agent.sample_minutes} minute mein PC ki halat note karta hai (RAM, CPU, disk).`)}</p></div>
      <Toggle bind:checked={s.agent.enabled} onchange={save} label="Monitor" /></div>
    <div class="setting"><div><strong>{t("Auto-clean", "Auto safai")}</strong><p class="muted small">{t("Scans at a set time and follows your approval setting.", "Tay samay par scan; aapki ijazat setting ke hisaab se.")}</p></div>
      <div class="row wrap">
        <select bind:value={s.agent.auto_clean.frequency} onchange={save}><option value="off">{t("Off", "Band")}</option><option value="daily">{t("Daily", "Roz")}</option><option value="weekly">{t("Weekly", "Hafte mein ek baar")}</option></select>
        {#if s.agent.auto_clean.frequency === "weekly"}<select bind:value={s.agent.auto_clean.weekday} onchange={save}>{#each DAYS as d, i}<option value={i}>{d}</option>{/each}</select>{/if}
        {#if s.agent.auto_clean.frequency !== "off"}<input type="time" bind:value={s.agent.auto_clean.time} onchange={save} />{/if}
      </div></div>
    <div class="setting"><div><strong>{t("Only when the PC is idle", "Sirf jab PC khaali ho")}</strong><p class="muted small">{t("Waits until the mouse and keyboard are unused for 5 minutes.", "5 minute tak mouse/keyboard na chale tab hi safai.")}</p></div>
      <Toggle bind:checked={s.agent.auto_clean.only_when_idle} onchange={save} label="Only when idle" /></div>
    <div class="setting"><div><strong>{t("Also when the disk is nearly full", "Disk bharne par bhi")}</strong><p class="muted small">{t("Starts a cleanup (with approval) when the system drive has less than 10% free.", "System drive 10% se kam khaali ho toh safai shuru (ijazat ke saath).")}</p></div>
      <Toggle bind:checked={s.agent.auto_clean.when_disk_low} onchange={save} label="When disk low" /></div>
    <div class="setting"><div><strong>{t("Daily report", "Daily report")}</strong><p class="muted small">{t("A short report of the day at this time.", "Roz is samay din ki chhoti report.")}</p></div>
      <div class="row"><Toggle checked={s.agent.daily_summary !== null} onchange={(v) => { s!.agent.daily_summary = v ? "21:00" : null; save(); }} label="Daily report" />
        {#if s.agent.daily_summary !== null}<input type="time" bind:value={s.agent.daily_summary} onchange={save} />{/if}</div></div>
    <div class="setting"><div><strong>{t("Weekly report", "Weekly report")}</strong><p class="muted small">{t("A summary of the whole week.", "Hafte bhar ka hisaab.")}</p></div>
      <div class="row wrap"><Toggle checked={s.agent.weekly_summary !== null} onchange={(v) => { s!.agent.weekly_summary = v ? { weekday: 6, time: "20:00" } : null; save(); }} label="Weekly report" />
        {#if s.agent.weekly_summary}<select bind:value={s.agent.weekly_summary.weekday} onchange={save}>{#each DAYS as d, i}<option value={i}>{d}</option>{/each}</select>
          <input type="time" bind:value={s.agent.weekly_summary.time} onchange={save} />{/if}</div></div>
    <div class="setting"><div><strong>{t("Smart alerts", "Smart alerts")}</strong><p class="muted small">{t("Only for lasting problems: RAM full for 15 min, disk nearly full, a new startup app, protection off.", "Sirf lambi chalne wali dikkat par: RAM 15 min se bhari, disk lagbhag full, nayi startup app, suraksha band.")}</p></div>
      <Toggle bind:checked={s.agent.smart_alerts} onchange={save} label="Smart alerts" /></div>
  </section>

  <section class="card">
    <div class="card-title"><Icon name="inbox" /><h2>Notifications</h2></div>
    <div class="setting"><div><strong>{t("Show notifications", "Notifications dikhayein")}</strong></div><Toggle bind:checked={s.agent.notify.enabled} onchange={save} label="Notifications" /></div>
    <div class="setting"><div><strong>{t("Sound", "Awaaz (sound)")}</strong><p class="muted small">{t("A soft chime when approval is needed.", "Ijazat maangte waqt halki si awaaz.")}</p></div><Toggle bind:checked={s.agent.notify.sound} onchange={save} label="Sound" /></div>
    <div class="setting"><div><strong>{t("Maximum per day", "Din mein zyada se zyada")}</strong><p class="muted small">{t("Limit for alerts (reports and approvals don't count).", "Alerts ki limit (reports aur ijazat isme nahi gine jate).")}</p></div>
      <input type="number" min="0" max="20" bind:value={s.agent.notify.max_per_day} onchange={save} style="width:80px" /></div>
    <div class="setting"><div><strong>{t("Quiet hours", "Shaanti ka samay")}</strong><p class="muted small">{t("No pop-ups in this window; messages still go to the inbox.", "Is beech pop-up nahi; messages inbox mein milenge.")}</p></div>
      <div class="row"><input type="time" bind:value={s.agent.notify.quiet_from} onchange={save} /><span class="muted">{t("to", "se")}</span><input type="time" bind:value={s.agent.notify.quiet_to} onchange={save} /></div></div>
  </section>

  <section class="card">
    <div class="card-title"><Icon name="bird" /><h2>{t("Offline AI", "Offline AI")}</h2><span class="spacer"></span>
      {#if ai}<span class="muted small">{t(`${bytes(ai.install.disk_bytes)} used · RAM ${bytes(ai.ram_total)}`, `${bytes(ai.install.disk_bytes)} jagah · RAM ${bytes(ai.ram_total)}`)}{ai.has_gpu ? t(" · graphics card found", " · graphics card mila") : ""}</span>{/if}</div>
    <p class="muted small" style="margin-bottom:12px">{t("The model loads only for a few seconds when needed (a report, a file check or a chat), then unloads. Everything stays on this PC; the internet is used once, for the download.", "Model sirf zarurat par kuch seconds ke liye load hota hai (summary, file check ya chat), phir band. Sab kuch isi PC par; internet sirf ek baar download ke liye.")}</p>
    {#if ai}
      <div class="models">
        {#each ai.models as m}
          {@const active = ai.enabled && ai.tier === m.tier && m.installed}
          <div class="model" class:active>
            <div class="row"><strong>{TIER_LABEL[m.tier]}</strong>
              {#if ai.recommended === m.tier}<span class="chip teal">{t("Best for this PC", "Aapke PC ke liye")}</span>{/if}
              {#if active}<span class="chip good">{t("In use", "Chalu")}</span>{/if}</div>
            <span class="mname">{m.name}</span>
            <p class="muted small">{m.description}</p>
            <span class="small">{bytes(m.size)} · {m.license}</span>
            {#if !m.fits}<span class="small warn">{t("May be heavy for this PC's RAM", "Is PC ki RAM ke liye bhari ho sakta hai")}</span>{/if}
            {#if downloading && progress?.tier === m.tier}
              <div class="bar"><span style="width:{progress.total ? (progress.done / progress.total) * 100 : 3}%"></span></div>
              <div class="row"><span class="small muted">{progress.stage === "runtime" ? "AI engine" : "Model"}: {bytes(progress.done)} / {bytes(progress.total)}</span><span class="spacer"></span>
                <button class="btn ghost small" onclick={() => api.aiInstall(m.tier, true)}>{t("Stop", "Roko")}</button></div>
            {:else if m.installed}
              <div class="row">{#if !active}<button class="btn small primary" onclick={() => useModel(m.tier)}>{t("Use", "Use karein")}</button>{/if}
                <button class="btn small ghost danger" onclick={() => remove(m.tier)}><Icon name="trash" size={15} />{t("Remove", "Hatao")}</button></div>
            {:else}
              <button class="btn small" class:primary={ai.recommended === m.tier} onclick={() => install(m.tier)} disabled={!!downloading}><Icon name="download" size={15} />Download</button>
            {/if}
          </div>
        {/each}
      </div>
      <div class="setting"><div><strong>{t("AI on", "AI chalu")}</strong></div><Toggle bind:checked={s.ai.enabled} onchange={save} label="AI" disabled={!ai.install.installed_tiers.length} /></div>
      <div class="setting"><div><strong>{t("Let the AI check unclear files", "Shaq wali files AI se jaanchwayein")}</strong><p class="muted small">{t("During auto-clean the AI looks at files the rules aren't sure about. If it calls a file risky, you are asked.", "Auto safai mein jin files ki pehchaan pakki na ho, AI dekhe. AI kisi file ko risky bata de toh aapse poocha jayega.")}</p></div>
        <Toggle bind:checked={s.ai.review_files} onchange={save} label="Review files" /></div>
      <div class="setting"><div><strong>{t("AI writes reports", "Reports AI likhe")}</strong><p class="muted small">{t("The AI writes the friendly part of reports; the numbers always come from the app.", "Daily/weekly report ka friendly hissa AI likhega; numbers hamesha app ke apne.")}</p></div>
        <Toggle bind:checked={s.ai.write_summaries} onchange={save} label="AI summaries" /></div>
      <div class="setting"><div><strong>{t("Use the graphics card", "Graphics card use karein")}</strong><p class="muted small">{t("Faster answers when one is available.", "Ho toh jawab tez aate hain.")}</p></div><Toggle bind:checked={s.ai.use_gpu} onchange={save} label="GPU" /></div>
      <div class="setting"><div><strong>{t("Unload after chat", "Chat ke baad model band karein")}</strong><p class="muted small">{t("Frees the RAM after this much idle time.", "Itni der kuch na poochein toh RAM wapas.")}</p></div>
        <select bind:value={s.ai.unload_after_secs} onchange={save}><option value={60}>1 min</option><option value={120}>2 min</option><option value={300}>5 min</option><option value={900}>15 min</option></select></div>
    {/if}
  </section>

  <p class="muted small about">{ui.info?.name ?? "Sweeplark"} {ui.info?.version} · {ui.info?.tagline} · {t("No ads, no account, no tracking.", "Na ads, na account, na tracking.")}</p>
{/if}

<style>
  .head { margin-bottom: 18px; }
  section.card { margin-bottom: 16px; }
  .setting { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 13px 0; border-top: 1px solid var(--line); }
  .setting > div:first-child { max-width: 560px; display: flex; flex-direction: column; gap: 2px; }
  .col-set { flex-direction: column; align-items: flex-start; }
  .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: var(--surface-3); flex: none; }
  .seg button { border: 0; background: transparent; padding: 6px 14px; border-radius: 999px; cursor: pointer; font-weight: 560; color: var(--slate); white-space: nowrap; }
  .seg button.on { background: var(--surface); color: var(--teal-text); box-shadow: var(--shadow); }
  code { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; }
  .models { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 8px; }
  .model { display: flex; flex-direction: column; gap: 6px; padding: 14px; border-radius: 14px; border: 1px solid var(--line); background: var(--surface-2); }
  .model.active { border-color: var(--teal); box-shadow: 0 0 0 3px var(--teal-soft); }
  .mname { font-size: 13px; color: var(--teal-text); font-weight: 600; }
  .warn { color: var(--amber-text); }
  .model .btn { align-self: flex-start; margin-top: auto; }
  .about { text-align: center; margin-top: 8px; }
</style>
