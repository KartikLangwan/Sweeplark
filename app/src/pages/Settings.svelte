<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Modal from "../components/Modal.svelte";
  import { api, on } from "../lib/api";
  import { bytes } from "../lib/format";
  import { applyTheme, saveSettings, t, toast, ui } from "../lib/state.svelte";
  import type { AiStatus, InstallProgress, Settings, Tier } from "../lib/types";

  let s = $state<Settings | null>(null);
  let ai = $state<AiStatus | null>(null);
  let progress = $state<InstallProgress | null>(null);
  let newPath = $state("");
  let newNever = $state("");
  let exportText = $state<string | null>(null);
  let importText = $state<string | null>(null);
  let report = $state<string | null>(null);
  let current = $state("general");
  const DAYS = $derived(ui.settings?.language === "hinglish"
    ? ["Somvar", "Mangalvar", "Budhvar", "Guruvar", "Shukravar", "Shanivar", "Ravivar"]
    : ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]);

  // Jump bar: each chip scrolls to the section with the matching id.
  const SECTIONS = $derived<[string, string][]>([
    ["general", t("General", "General")],
    ["trust", t("Trust", "Bharosa")],
    ["automation", t("Automation", "Automatic kaam")],
    ["notifications", t("Notifications", "Notifications")],
    ["boost", t("Boost & Optimizer", "Boost aur Optimizer")],
    ["ai", "AI"],
    ["backup", t("Backup & help", "Backup aur madad")],
    ["about", t("About", "About")],
  ]);
  function go(id: string) {
    current = id;
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  onMount(() => {
    api.settings().then((x) => (s = structuredClone(x)));
    loadAi();
    const u = on<InstallProgress>("ai-progress", (p) => {
      progress = p;
      if (p.stage === "done") { toast(t("AI model is ready", "AI model taiyaar hai")); loadAi(); api.settings().then((x) => (s = structuredClone(x))); }
      if (p.stage === "error") toast(p.error ?? t("Download failed", "Download nahi hua"), "warn");
    });
    // The tray or an AI download changed settings: show the new values.
    const c = on("settings-changed", () => api.settings().then((x) => (s = structuredClone(x))));
    return () => { u.then((f) => f()); c.then((f) => f()); };
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
      try {
        const saved = await saveSettings($state.snapshot(s) as Settings);
        s.rev = saved.rev;
        toast(t("Settings saved", "Settings save ho gayi"));
      } catch (e) {
        toast(e instanceof Error ? e.message : String(e), "warn");
        if (ui.settings) s = structuredClone($state.snapshot(ui.settings) as Settings);
      }
    }, 350);
  }

  async function install(tier: Tier) {
    try { await api.aiInstall(tier); progress = { stage: "model", tier, done: 0, total: 1, error: null }; }
    catch (e) { toast(String(e), "warn"); }
  }
  let removing = $state<Tier | null>(null);
  async function remove(tier: Tier) {
    removing = null;
    try {
      await api.aiRemove(tier);
      toast(t("Model removed", "Model hata diya"));
      await loadAi();
      s = structuredClone(await api.settings());
    } catch (e) { toast(String(e), "warn"); }
  }
  function useModel(tier: Tier) {
    if (!s) return;
    s.ai.tier = tier; s.ai.enabled = true; save(); loadAi();
  }
  const downloading = $derived(progress && (progress.stage === "runtime" || progress.stage === "model"));
  let checking = $state(false);
  async function checkNow() {
    checking = true;
    try {
      const u = await api.updateCheck(true);
      toast(u ? t(`Version ${u.version} is available`, `Version ${u.version} aaya hai`) : t("You have the latest version", "Aapke paas latest version hai"), u ? "info" : "good");
      if (u) window.dispatchEvent(new CustomEvent("sweeplark-update", { detail: u }));
    } catch (e) { toast(String(e), "warn"); } finally { checking = false; }
  }
  async function openExport() {
    try { exportText = await api.settingsExport(); } catch (e) { toast(String(e), "warn"); }
  }
  async function copy(text: string) {
    try { await navigator.clipboard.writeText(text); toast(t("Copied", "Copy ho gaya")); } catch { toast(t("Select the text and press Ctrl+C", "Text select karke Ctrl+C dabayein"), "info"); }
  }
  async function doImport() {
    if (!importText) return;
    try {
      const x = await api.settingsImport(importText);
      s = structuredClone(x); ui.settings = x; importText = null;
      toast(t("Settings imported", "Settings import ho gayi"));
    } catch (e) { toast(String(e), "warn"); }
  }
  async function openReport() {
    try { report = await api.bugReport(); } catch (e) { toast(String(e), "warn"); }
  }
  function sendReport() {
    if (!report) return;
    const body = encodeURIComponent(report.slice(0, 6000));
    api.openUrl(`https://github.com/${ui.info?.repo ?? "KartikLangwan/sweeplark"}/issues/new?title=${encodeURIComponent("Bug: ")}&body=${body}`);
  }
  const TIER_LABEL: Record<Tier, string> = { lite: "Lite", balanced: "Balanced", power: "High" };
</script>

<header class="head"><div><h1>{t("Settings", "Settings")}</h1><p class="muted">{t("You are in control. Changes save instantly.", "Har cheez aapke control mein. Badlav turant save hote hain.")}</p></div></header>

{#if s}
  <nav class="jump" aria-label={t("Jump to a section", "Kisi section par jaayein")}>
    {#each SECTIONS as [id, label]}
      <button class:on={current === id} onclick={() => go(id)}>{label}</button>
    {/each}
  </nav>

  <section class="card sec" id="general">
    <div class="card-title"><Icon name="sparkle" /><h2>{t("General", "General")}</h2></div>
    <div class="setting"><div><strong>{t("Language", "Bhasha (Language)")}</strong><p class="muted small">{t("Used across the app, notifications, reports and the assistant.", "Poore app, notifications, reports aur assistant mein.")}</p></div>
      <div class="seg"><button class:on={s.language === "en"} onclick={() => { s!.language = "en"; save(); }}>English</button>
        <button class:on={s.language === "hinglish"} onclick={() => { s!.language = "hinglish"; save(); }}>Hinglish</button></div></div>
    <div class="setting"><div><strong>{t("Theme", "Theme")}</strong><p class="muted small">{t("Soft light or dark. The first start follows Windows.", "Soft light ya dark. Pehli baar Windows ke hisaab se.")}</p></div>
      <div class="seg">{#each [["light", t("Light", "Light")], ["dark", t("Dark", "Dark")]] as [id, label]}<button class:on={ui.theme === id} onclick={() => applyTheme(id as "light" | "dark")}>{label}</button>{/each}</div></div>
  </section>

  <section class="card sec" id="trust">
    <div class="card-title"><Icon name="shield" /><h2>{t("Trust and approvals", "Bharosa aur ijazat")}</h2></div>
    <div class="setting"><div><strong>{t("Trust level", "Bharose ka level")}</strong><p class="muted small">{t("Safe: only safe items are shown. Ask: items that ask first too. Expert: everything (always asks).", "Safe: sirf safe cheezein dikhengi. Ask: poochne wali bhi. Expert: sab kuch (hamesha pooch kar).")}</p></div>
      <div class="seg">{#each ["safe", "ask", "expert"] as lvl}<button class:on={s.trust_level === lvl} onclick={() => { s!.trust_level = lvl as Settings["trust_level"]; save(); }}>{lvl === "safe" ? t("Safe", "Safe") : lvl === "ask" ? t("Ask", "Ask") : t("Expert", "Expert")}</button>{/each}</div></div>
    <div class="setting"><div><strong>{t("Approval before auto-clean", "Auto safai se pehle ijazat")}</strong><p class="muted small">{t("Always ask (default): nothing is removed without your yes. Risky only: cache and temp are cleaned automatically, the rest asks with a notification. Files that look like documents or photos always ask.", "Har baar poocho (default): bina aapki haan ke kuch nahi hatega. Sirf risky: cache/temp khud saaf, baaki ke liye notification. Documents/photos jaisi files par hamesha poochega.")}</p></div>
      <div class="seg"><button class:on={s.agent.approval === "ask_always"} onclick={() => { s!.agent.approval = "ask_always"; save(); }}>{t("Always ask", "Har baar poocho")}</button>
        <button class:on={s.agent.approval === "risky_only"} onclick={() => { s!.agent.approval = "risky_only"; save(); }}>{t("Risky only", "Sirf risky par")}</button></div></div>
    <div class="setting"><div><strong>{t("Browser history and cookies", "Browser history aur cookies")}</strong><p class="muted small">{t("Privacy cleaners appear only when this is on. Removing cookies signs you out of websites.", "Chalu karne par hi privacy cleaners dikhenge. Cookies hatane se websites se logout ho jayenge.")}</p></div>
      <Toggle bind:checked={s.privacy_consent} onchange={save} label={t("Privacy cleaners", "Privacy cleaners")} /></div>
    <div class="setting"><div><strong>{t("Recycle Bin", "Recycle Bin")}</strong><p class="muted small">{t("Only items older than this are removed.", "Itne din se purani cheezein hi hatengi.")}</p></div>
      <div class="row"><input type="number" min="0" max="365" bind:value={s.recycle_bin_min_age_days} onchange={save} /><span class="muted">{t("days", "din")}</span></div></div>
    <div class="setting col-set"><div><strong>{t("Never touch (whitelist)", "Kabhi na chhuein (whitelist)")}</strong><p class="muted small">{t("The cleaner never touches these folders or files.", "Ye folders/files cleaner kabhi nahi chhuega.")}</p></div>
      <div class="col" style="width:100%">
        {#each s.whitelist as w, i}<div class="row"><code class="selectable">{w}</code><span class="spacer"></span><button class="btn ghost small" onclick={() => { s!.whitelist.splice(i, 1); save(); }}>{t("Remove", "Hatao")}</button></div>{/each}
        <div class="row"><input type="text" bind:value={newPath} placeholder="C:\Users\you\AppData\Local\Temp\MyProject" style="flex:1" />
          <button class="btn small" onclick={() => { if (newPath.trim()) { s!.whitelist.push(newPath.trim()); newPath = ""; save(); } }}>{t("Add", "Jodo")}</button></div>
      </div></div>
  </section>

  <section class="card sec" id="automation">
    <div class="card-title"><Icon name="restart" /><h2>{t("Automation", "Automatic kaam")}</h2></div>
    <div class="setting"><div><strong>{t("Start with Windows", "Windows ke saath chalu ho")}</strong><p class="muted small">{t("Runs quietly in the tray. The background monitor uses no AI and almost no CPU.", "Tray mein chupchaap chalega. Background monitor bina AI ke, lagbhag 0% CPU.")}</p></div>
      <Toggle bind:checked={s.agent.start_with_windows} onchange={save} label={t("Start with Windows", "Windows ke saath chalu ho")} /></div>
    <div class="setting"><div><strong>{t("Background monitor", "Background monitor")}</strong><p class="muted small">{t(`Notes RAM, CPU and disk every ${s.agent.sample_minutes} minutes.`, `Har ${s.agent.sample_minutes} minute mein PC ki halat note karta hai (RAM, CPU, disk).`)}</p></div>
      <Toggle bind:checked={s.agent.enabled} onchange={save} label={t("Background monitor", "Background monitor")} /></div>
    <div class="setting"><div><strong>{t("Auto-clean", "Auto safai")}</strong><p class="muted small">{t("Scans at a set time and follows your approval setting.", "Tay samay par scan; aapki ijazat setting ke hisaab se.")}</p></div>
      <div class="row wrap">
        <select bind:value={s.agent.auto_clean.frequency} onchange={save}><option value="off">{t("Off", "Band")}</option><option value="daily">{t("Daily", "Roz")}</option><option value="weekly">{t("Weekly", "Hafte mein ek baar")}</option></select>
        {#if s.agent.auto_clean.frequency === "weekly"}<select bind:value={s.agent.auto_clean.weekday} onchange={save}>{#each DAYS as d, i}<option value={i}>{d}</option>{/each}</select>{/if}
        {#if s.agent.auto_clean.frequency !== "off"}<input type="time" bind:value={s.agent.auto_clean.time} onchange={save} />{/if}
      </div></div>
    <div class="setting"><div><strong>{t("Only when the PC is idle", "Sirf jab PC khaali ho")}</strong><p class="muted small">{t("Waits until the mouse and keyboard are unused for 5 minutes.", "5 minute tak mouse/keyboard na chale tab hi safai.")}</p></div>
      <Toggle bind:checked={s.agent.auto_clean.only_when_idle} onchange={save} label={t("Only when the PC is idle", "Sirf jab PC khaali ho")} /></div>
    <div class="setting"><div><strong>{t("Also when the disk is nearly full", "Disk bharne par bhi")}</strong><p class="muted small">{t("Starts a cleanup (with approval) when the system drive has less than 10% free.", "System drive 10% se kam khaali ho toh safai shuru (ijazat ke saath).")}</p></div>
      <Toggle bind:checked={s.agent.auto_clean.when_disk_low} onchange={save} label={t("Also when the disk is nearly full", "Disk bharne par bhi")} /></div>
    <div class="setting"><div><strong>{t("Daily report", "Daily report")}</strong><p class="muted small">{t("A short report of the day at this time.", "Roz is samay din ki chhoti report.")}</p></div>
      <div class="row"><Toggle controlled checked={s.agent.daily_summary !== null} onchange={(v) => { s!.agent.daily_summary = v ? "21:00" : null; save(); }} label={t("Daily report", "Daily report")} />
        {#if s.agent.daily_summary !== null}<input type="time" bind:value={s.agent.daily_summary} onchange={save} />{/if}</div></div>
    <div class="setting"><div><strong>{t("Weekly report", "Weekly report")}</strong><p class="muted small">{t("A summary of the whole week.", "Hafte bhar ka hisaab.")}</p></div>
      <div class="row wrap"><Toggle controlled checked={s.agent.weekly_summary !== null} onchange={(v) => { s!.agent.weekly_summary = v ? { weekday: 6, time: "20:00" } : null; save(); }} label={t("Weekly report", "Weekly report")} />
        {#if s.agent.weekly_summary}<select bind:value={s.agent.weekly_summary.weekday} onchange={save}>{#each DAYS as d, i}<option value={i}>{d}</option>{/each}</select>
          <input type="time" bind:value={s.agent.weekly_summary.time} onchange={save} />{/if}</div></div>
    <div class="setting"><div><strong>{t("Smart alerts", "Smart alerts")}</strong><p class="muted small">{t("Only for lasting problems: RAM full for 15 min, disk nearly full, a new startup app, protection off.", "Sirf lambi chalne wali dikkat par: RAM 15 min se bhari, disk lagbhag full, nayi startup app, suraksha band.")}</p></div>
      <Toggle bind:checked={s.agent.smart_alerts} onchange={save} label={t("Smart alerts", "Smart alerts")} /></div>
  </section>

  <section class="card sec" id="notifications">
    <div class="card-title"><Icon name="inbox" /><h2>{t("Notifications", "Notifications")}</h2></div>
    <div class="setting"><div><strong>{t("Show notifications", "Notifications dikhayein")}</strong></div><Toggle bind:checked={s.agent.notify.enabled} onchange={save} label={t("Notifications", "Notifications")} /></div>
    <div class="setting"><div><strong>{t("Sound", "Awaaz (sound)")}</strong><p class="muted small">{t("A soft chime when approval is needed.", "Ijazat maangte waqt halki si awaaz.")}</p></div><Toggle bind:checked={s.agent.notify.sound} onchange={save} label={t("Sound", "Awaaz (sound)")} /></div>
    <div class="setting"><div><strong>{t("Maximum per day", "Din mein zyada se zyada")}</strong><p class="muted small">{t("Limit for alerts (reports and approvals don't count).", "Alerts ki limit (reports aur ijazat isme nahi gine jate).")}</p></div>
      <input type="number" min="0" max="20" bind:value={s.agent.notify.max_per_day} onchange={save} /></div>
    <div class="setting"><div><strong>{t("Quiet hours", "Shaanti ka samay")}</strong><p class="muted small">{t("No pop-ups in this window; messages still go to the inbox.", "Is beech pop-up nahi; messages inbox mein milenge.")}</p></div>
      <div class="row"><input type="time" bind:value={s.agent.notify.quiet_from} onchange={save} /><span class="muted">{t("to", "se")}</span><input type="time" bind:value={s.agent.notify.quiet_to} onchange={save} /></div></div>
  </section>

  <section class="card sec" id="boost">
    <div class="card-title"><Icon name="rocket" /><h2>{t("Boost and Optimizer", "Boost aur Optimizer")}</h2></div>
    <div class="setting"><div><strong>{t("Restore point before Optimizer changes", "Optimizer se pehle restore point")}</strong><p class="muted small">{t("Before the first system-level tweak of a session, Windows saves a restore point (when running as administrator).", "Har session ke pehle system tweak se pehle Windows ek restore point banata hai (admin mode mein).")}</p></div>
      <Toggle bind:checked={s.restore_point_before_tweaks} onchange={save} label={t("Restore point before Optimizer changes", "Optimizer se pehle restore point")} /></div>
    <div class="setting"><div><strong>{t("Game mode", "Game mode")}</strong><p class="muted small">{t("Calm background apps while a game runs full screen; back to normal when it closes.", "Game full screen chale tab background apps shaant; band hote hi normal.")}</p></div>
      <Toggle bind:checked={s.boost.auto_game} onchange={save} label={t("Game mode", "Game mode")} /></div>
    <div class="setting"><div><strong>{t("RAM guard", "RAM guard")}</strong><p class="muted small">{t(`Free RAM from background apps when RAM stays above ${s.boost.ram_guard_percent}%.`, `RAM ${s.boost.ram_guard_percent}% se upar rahe toh background apps ki RAM khaali.`)}</p></div>
      <Toggle bind:checked={s.boost.ram_guard} onchange={save} label={t("RAM guard", "RAM guard")} /></div>
    <div class="setting col-set"><div><strong>{t("Never touch these apps", "In apps ko kabhi mat chhuo")}</strong><p class="muted small">{t("Boost, Game mode and RAM guard leave them alone (name without .exe).", "Boost, Game mode aur RAM guard inhe nahi chhuenge (naam bina .exe).")}</p></div>
      <div class="col" style="width:100%">
        <div class="row wrap">{#each s.boost.never as n, i}<span class="chip">{n}<button class="x" aria-label={t(`Remove ${n}`, `${n} hatao`)} onclick={() => { s!.boost.never.splice(i, 1); save(); }}>×</button></span>{:else}<span class="muted small">{t("None yet. Voice chat, streaming, anti-cheat and security apps are always protected.", "Abhi koi nahi. Voice chat, streaming, anti-cheat aur security apps hamesha surakshit hain.")}</span>{/each}</div>
        <div class="row"><input type="text" bind:value={newNever} placeholder="e.g. HandBrake" style="flex:1" />
          <button class="btn small" onclick={() => { const n = newNever.trim().replace(/\.exe$/i, ""); if (n) { s!.boost.never.push(n); newNever = ""; save(); } }}>{t("Add", "Jodo")}</button></div>
      </div></div>
  </section>

  {#if ai}
    <section class="card sec" id="ai">
      <div class="card-title"><Icon name="bird" /><h2>{t("Offline AI", "Offline AI")}</h2><span class="spacer"></span>
        <span class="muted small">{t(`${bytes(ai.install.disk_bytes)} used · RAM ${bytes(ai.ram_total)}`, `${bytes(ai.install.disk_bytes)} jagah · RAM ${bytes(ai.ram_total)}`)}{ai.has_gpu ? t(" · graphics card found", " · graphics card mila") : ""}</span></div>
      <p class="muted small" style="margin-bottom:12px">{t("The model loads only for a few seconds when needed (a report, a file check or a chat), then unloads. Everything stays on this PC; the internet is used once, for the download.", "Model sirf zarurat par kuch seconds ke liye load hota hai (summary, file check ya chat), phir band. Sab kuch isi PC par; internet sirf ek baar download ke liye.")}</p>
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
              <div class="row"><span class="small muted">{progress.stage === "runtime" ? t("AI engine", "AI engine") : t("Model", "Model")}: {progress.total ? `${Math.floor((progress.done / progress.total) * 100)}% · ` : ""}{bytes(progress.done)} / {bytes(progress.total)}</span><span class="spacer"></span>
                <button class="btn ghost small" onclick={() => api.aiInstall(m.tier, true)}>{t("Stop", "Roko")}</button></div>
            {:else if m.installed}
              <div class="row">{#if !active}<button class="btn small primary" onclick={() => useModel(m.tier)}>{t("Use", "Use karein")}</button>{/if}
                <button class="btn small ghost danger" onclick={() => (removing = m.tier)}><Icon name="trash" size={15} />{t("Remove", "Hatao")}</button></div>
            {:else}
              <button class="btn small" class:primary={ai.recommended === m.tier} onclick={() => install(m.tier)} disabled={!!downloading}><Icon name="download" size={15} />{t("Download", "Download")}</button>
            {/if}
          </div>
        {/each}
      </div>
      <div class="setting"><div><strong>{t("AI on", "AI chalu")}</strong></div><Toggle bind:checked={s.ai.enabled} onchange={save} label={t("AI on", "AI chalu")} disabled={!ai.install.installed_tiers.length} /></div>
      <div class="setting"><div><strong>{t("Let the AI check unclear files", "Shaq wali files AI se jaanchwayein")}</strong><p class="muted small">{t("During auto-clean the AI looks at files the rules aren't sure about. If it calls a file risky, you are asked.", "Auto safai mein jin files ki pehchaan pakki na ho, AI dekhe. AI kisi file ko risky bata de toh aapse poocha jayega.")}</p></div>
        <Toggle bind:checked={s.ai.review_files} onchange={save} label={t("Let the AI check unclear files", "Shaq wali files AI se jaanchwayein")} /></div>
      <div class="setting"><div><strong>{t("AI writes reports", "Reports AI likhe")}</strong><p class="muted small">{t("The AI writes the friendly part of reports; the numbers always come from the app.", "Daily/weekly report ka friendly hissa AI likhega; numbers hamesha app ke apne.")}</p></div>
        <Toggle bind:checked={s.ai.write_summaries} onchange={save} label={t("AI writes reports", "Reports AI likhe")} /></div>
      <div class="setting"><div><strong>{t("Use the graphics card", "Graphics card use karein")}</strong><p class="muted small">{t("Faster answers when one is available.", "Ho toh jawab tez aate hain.")}</p></div><Toggle bind:checked={s.ai.use_gpu} onchange={save} label={t("Use the graphics card", "Graphics card use karein")} /></div>
      <div class="setting"><div><strong>{t("Unload after chat", "Chat ke baad model band karein")}</strong><p class="muted small">{t("Frees the RAM after this much idle time.", "Itni der kuch na poochein toh RAM wapas.")}</p></div>
        <select bind:value={s.ai.unload_after_secs} onchange={save}><option value={60}>{t("1 min", "1 min")}</option><option value={120}>{t("2 min", "2 min")}</option><option value={300}>{t("5 min", "5 min")}</option><option value={900}>{t("15 min", "15 min")}</option></select></div>
    </section>
  {/if}

  <section class="card sec" id="backup">
    <div class="card-title"><Icon name="bug" /><h2>{t("Backup and help", "Backup aur madad")}</h2></div>
    <div class="setting"><div><strong>{t("Copy settings to another PC", "Settings doosre PC par le jaayein")}</strong><p class="muted small">{t("Export gives you text to copy; Import takes it back.", "Export copy karne ke liye text deta hai; Import use wapas leta hai.")}</p></div>
      <div class="row"><button class="btn small" onclick={openExport}><Icon name="copy" size={14} />{t("Export", "Export")}</button><button class="btn small" onclick={() => (importText = "")}><Icon name="download" size={14} />{t("Import", "Import")}</button></div></div>
    <div class="setting"><div><strong>{t("Report a problem", "Problem batayein")}</strong><p class="muted small">{t("Opens a GitHub issue with your Sweeplark and Windows version and recent log lines (your user name is hidden). You see everything before sending.", "GitHub issue khulega: Sweeplark aur Windows version aur haal ke log (user name chhupa hua). Bhejne se pehle sab dekh sakte hain.")}</p></div>
      <div class="row"><button class="btn small" onclick={() => api.openLogs().catch((e) => toast(String(e), "warn"))}>{t("Open logs", "Logs kholo")}</button><button class="btn small primary" onclick={openReport}><Icon name="bug" size={14} />{t("Report", "Report")}</button></div></div>
  </section>

  <section class="card sec about-card" id="about">
    <div class="card-title"><Icon name="bird" /><h2>{t("About", "About")}</h2></div>
    <div class="row app-row">
      <div class="col" style="gap:2px"><strong>Sweeplark {ui.info?.version}{ui.info?.portable ? t(" · Portable", " · Portable") : ""}</strong>
        <span class="muted small">{ui.info?.tagline}</span></div>
      <span class="spacer"></span>
      <div class="row wrap"><span class="muted small">{t("Check at startup", "Startup par check")}</span><Toggle bind:checked={s.check_updates} onchange={save} label={t("Check at startup", "Startup par check")} />
        <button class="btn small" onclick={checkNow} disabled={checking}><Icon name="download" size={15} />{checking ? t("Checking…", "Check ho raha…") : t("Check for updates", "Update check karo")}</button></div>
    </div>
    <div class="dev">
      <div class="avatar">KL</div>
      <div class="col" style="gap:3px">
        <span class="muted small">{t("Made by", "Banaya")}</span>
        <strong class="dname">{ui.info?.developer ?? "Kartik Langwan"}</strong>
        <span class="muted small">{ui.info?.developer_title}</span>
        <p class="small">{ui.info?.developer_tagline}</p>
      </div>
      <span class="spacer"></span>
      <button class="btn primary small" onclick={() => api.openUrl(ui.info?.developer_url ?? "https://kartiklangwan.runs-on.dev")}>{t("Visit portfolio", "Portfolio dekhein")}<Icon name="chevron" size={14} /></button>
    </div>
  </section>

  <p class="muted small about">{ui.info?.name ?? "Sweeplark"} {ui.info?.version} · {ui.info?.tagline} · {t("No ads, no account, no tracking.", "Na ads, na account, na tracking.")}</p>
{/if}

<Modal open={removing !== null} onclose={() => (removing = null)} title={t("Remove this AI model?", "Yeh AI model hatayein?")}>
  <p class="muted">{t("It frees the disk space. You can download it again later.", "Disk ki jagah khaali hogi. Baad mein dobara download kar sakte hain.")}</p>
  {#snippet footer()}<button class="btn" onclick={() => (removing = null)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={() => removing && remove(removing)}>{t("Remove", "Hatao")}</button>{/snippet}
</Modal>
<Modal open={exportText !== null} onclose={() => (exportText = null)} title={t("Your settings", "Aapki settings")} wide>
  <textarea readonly rows="12" class="mono">{exportText}</textarea>
  {#snippet footer()}<button class="btn" onclick={() => (exportText = null)}>{t("Close", "Band")}</button><button class="btn primary" onclick={() => exportText && copy(exportText)}><Icon name="copy" size={14} />{t("Copy", "Copy")}</button>{/snippet}
</Modal>
<Modal open={importText !== null} onclose={() => (importText = null)} title={t("Import settings", "Settings import")} wide>
  <p class="muted small" style="margin-bottom:8px">{t("Paste the text you exported from Sweeplark.", "Sweeplark se export kiya text yahan paste karein.")}</p>
  <textarea rows="12" class="mono" bind:value={importText}></textarea>
  {#snippet footer()}<button class="btn" onclick={() => (importText = null)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={doImport} disabled={!importText?.trim()}>{t("Import", "Import")}</button>{/snippet}
</Modal>
<Modal open={report !== null} onclose={() => (report = null)} title={t("Report a problem", "Problem batayein")} wide>
  <p class="muted small" style="margin-bottom:8px">{t("Write what happened under \"What happened\". You can edit everything.", "\"What happened\" ke neeche likhiye kya hua. Sab kuch badal sakte hain.")}</p>
  <textarea rows="14" class="mono" bind:value={report}></textarea>
  {#snippet footer()}<button class="btn" onclick={() => (report = null)}>{t("Cancel", "Ruko")}</button><button class="btn" onclick={() => report && copy(report)}><Icon name="copy" size={14} />{t("Copy", "Copy")}</button><button class="btn primary" onclick={sendReport}>{t("Open on GitHub", "GitHub par kholo")}</button>{/snippet}
</Modal>

<style>
  .mono { width: 100%; font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; border-radius: 10px; border: 1px solid var(--line); background: var(--surface-2); color: var(--ink); padding: 10px; resize: vertical; }
  .chip .x { border: 0; background: none; cursor: pointer; color: inherit; margin-left: 4px; font-size: 14px; line-height: 1; }
  code { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12px; }

  /* Jump bar: sticks to the top of the scrolling page and spans its full width. */
  .jump { position: sticky; top: 0; z-index: 4; display: flex; gap: 6px; flex-wrap: wrap; margin: 0 -36px 18px; padding: 10px 36px; background: var(--surface-2); border-bottom: 1px solid var(--line); }
  .jump button { height: 30px; padding: 0 12px; border-radius: 999px; border: 1px solid var(--line); background: var(--surface); color: var(--ink-2); cursor: pointer; font-size: 12.5px; font-weight: 560; white-space: nowrap; }
  .jump button:hover { background: var(--surface-3); }
  .jump button.on { background: var(--teal-soft); border-color: var(--teal); color: var(--teal-text); }
  .jump button:focus-visible { outline: 2px solid var(--teal); outline-offset: 2px; }

  /* Sections: keep the heading clear of the sticky jump bar when scrolled to. */
  .sec { margin-bottom: 16px; scroll-margin-top: 64px; }
  .sec .card-title { margin-bottom: 6px; }

  /* Rows: label on the left (takes the room), control on the right with consistent widths. */
  .setting { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 14px 0; border-top: 1px solid var(--line); }
  .card-title + .setting { border-top: 0; padding-top: 6px; }
  .setting > div:first-child { flex: 1; min-width: 0; max-width: 600px; display: flex; flex-direction: column; gap: 2px; }
  .setting > :not(:first-child) { flex: none; }
  .setting input[type="number"] { width: 88px; }
  .setting input[type="time"] { width: 124px; }
  .setting select { min-width: 150px; }
  .col-set { flex-direction: column; align-items: flex-start; }
  .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: var(--surface-3); flex: none; }
  .seg button { border: 0; background: transparent; padding: 6px 14px; border-radius: 999px; cursor: pointer; font-weight: 560; color: var(--slate); white-space: nowrap; }
  .seg button.on { background: var(--surface); color: var(--teal-text); box-shadow: var(--shadow); }

  .models { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 6px; }
  .model { display: flex; flex-direction: column; gap: 6px; padding: 14px; border-radius: 14px; border: 1px solid var(--line); background: var(--surface-2); }
  .model.active { border-color: var(--teal); box-shadow: 0 0 0 3px var(--teal-soft); }
  .mname { font-size: 13px; color: var(--teal-text); font-weight: 600; }
  .warn { color: var(--amber-text); }
  .model .btn { align-self: flex-start; margin-top: auto; }

  .about { text-align: center; margin-top: 8px; }
  .app-row { padding: 6px 0 14px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  .dev { display: flex; align-items: center; gap: 16px; padding-top: 16px; flex-wrap: wrap; }
  .avatar { width: 54px; height: 54px; border-radius: 16px; display: grid; place-items: center; font-weight: 700; font-size: 18px; color: #fff; background: linear-gradient(135deg, var(--mint), var(--teal)); flex: none; }
  .dname { font-size: 18px; }
</style>
