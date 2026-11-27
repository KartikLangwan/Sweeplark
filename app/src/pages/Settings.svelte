<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { api, on } from "../lib/api";
  import { bytes } from "../lib/format";
  import { saveSettings, toast, ui } from "../lib/state.svelte";
  import type { AiStatus, InstallProgress, Settings, Tier } from "../lib/types";

  let s = $state<Settings | null>(null);
  let ai = $state<AiStatus | null>(null);
  let progress = $state<InstallProgress | null>(null);
  let newPath = $state("");
  const DAYS = ["Somvar", "Mangalvar", "Budhvar", "Guruvar", "Shukravar", "Shanivar", "Ravivar"];

  onMount(() => {
    api.settings().then((x) => (s = structuredClone(x)));
    loadAi();
    const u = on<InstallProgress>("ai-progress", (p) => {
      progress = p;
      if (p.stage === "done") { toast("AI model taiyaar hai"); loadAi(); api.settings().then((x) => (s = structuredClone(x))); }
      if (p.stage === "error") toast(p.error ?? "Download nahi hua", "warn");
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
      try { await saveSettings($state.snapshot(s) as Settings); toast("Settings save ho gayi"); } catch (e) { toast(String(e), "warn"); }
    }, 350);
  }

  async function install(tier: Tier) {
    try { await api.aiInstall(tier); progress = { stage: "model", tier, done: 0, total: 1, error: null }; }
    catch (e) { toast(String(e), "warn"); }
  }
  async function remove(tier: Tier) {
    await api.aiRemove(tier);
    toast("Model hata diya");
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

<header class="head"><div><h1>Settings</h1><p class="muted">Har cheez aapke control mein. Badlav turant save hote hain.</p></div></header>

{#if s}
  <section class="card">
    <div class="card-title"><Icon name="shield" /><h2>Bharosa aur ijazat</h2></div>
    <div class="setting"><div><strong>Bharose ka level</strong><p class="muted small">Safe: sirf safe cheezein dikhengi. Ask: poochne wali bhi. Expert: sab kuch (hamesha pooch kar).</p></div>
      <div class="seg">{#each ["safe", "ask", "expert"] as t}<button class:on={s.trust_level === t} onclick={() => { s!.trust_level = t as Settings["trust_level"]; save(); }}>{t === "safe" ? "Safe" : t === "ask" ? "Ask" : "Expert"}</button>{/each}</div></div>
    <div class="setting"><div><strong>Auto safai se pehle ijazat</strong><p class="muted small">"Har baar poocho" (default): bina aapki haan ke kuch nahi hatega. "Sirf risky": cache/temp khud saaf, baaki ke liye notification. Documents/photos jaisi files par hamesha poochega.</p></div>
      <div class="seg"><button class:on={s.agent.approval === "ask_always"} onclick={() => { s!.agent.approval = "ask_always"; save(); }}>Har baar poocho</button>
        <button class:on={s.agent.approval === "risky_only"} onclick={() => { s!.agent.approval = "risky_only"; save(); }}>Sirf risky par</button></div></div>
    <div class="setting"><div><strong>Browser history aur cookies</strong><p class="muted small">Chalu karne par hi privacy cleaners dikhenge. Cookies hatane se websites se logout ho jayenge.</p></div>
      <Toggle bind:checked={s.privacy_consent} onchange={save} label="Privacy cleaners" /></div>
    <div class="setting"><div><strong>Recycle Bin</strong><p class="muted small">Itne din se purani cheezein hi hatengi.</p></div>
      <div class="row"><input type="number" min="0" max="365" bind:value={s.recycle_bin_min_age_days} onchange={save} style="width:80px" /><span class="muted">din</span></div></div>
    <div class="setting col-set"><div><strong>Kabhi na chhuein (whitelist)</strong><p class="muted small">Ye folders/files cleaner kabhi nahi chhuega.</p></div>
      <div class="col" style="width:100%">
        {#each s.whitelist as w, i}<div class="row"><code class="selectable">{w}</code><span class="spacer"></span><button class="btn ghost small" onclick={() => { s!.whitelist.splice(i, 1); save(); }}>Hatao</button></div>{/each}
        <div class="row"><input type="text" bind:value={newPath} placeholder="C:\Users\aap\AppData\Local\Temp\MeraProject" style="flex:1" />
          <button class="btn small" onclick={() => { if (newPath.trim()) { s!.whitelist.push(newPath.trim()); newPath = ""; save(); } }}>Jodo</button></div>
      </div></div>
  </section>

  <section class="card">
    <div class="card-title"><Icon name="restart" /><h2>Automatic kaam</h2></div>
    <div class="setting"><div><strong>Windows ke saath chalu ho</strong><p class="muted small">Tray mein chupchaap chalega. Background monitor bina AI ke, lagbhag 0% CPU.</p></div>
      <Toggle bind:checked={s.agent.start_with_windows} onchange={save} label="Start with Windows" /></div>
    <div class="setting"><div><strong>Background monitor</strong><p class="muted small">Har {s.agent.sample_minutes} minute mein PC ki halat note karta hai (RAM, CPU, disk).</p></div>
      <Toggle bind:checked={s.agent.enabled} onchange={save} label="Monitor" /></div>
    <div class="setting"><div><strong>Auto safai</strong><p class="muted small">Tay samay par scan; aapki ijazat setting ke hisaab se.</p></div>
      <div class="row wrap">
        <select bind:value={s.agent.auto_clean.frequency} onchange={save}><option value="off">Band</option><option value="daily">Roz</option><option value="weekly">Hafte mein ek baar</option></select>
        {#if s.agent.auto_clean.frequency === "weekly"}<select bind:value={s.agent.auto_clean.weekday} onchange={save}>{#each DAYS as d, i}<option value={i}>{d}</option>{/each}</select>{/if}
        {#if s.agent.auto_clean.frequency !== "off"}<input type="time" bind:value={s.agent.auto_clean.time} onchange={save} />{/if}
      </div></div>
    <div class="setting"><div><strong>Sirf jab PC khaali ho</strong><p class="muted small">5 minute tak mouse/keyboard na chale tab hi safai.</p></div>
      <Toggle bind:checked={s.agent.auto_clean.only_when_idle} onchange={save} label="Only when idle" /></div>
    <div class="setting"><div><strong>Disk bharne par bhi</strong><p class="muted small">System drive 10% se kam khaali ho toh safai shuru (ijazat ke saath).</p></div>
      <Toggle bind:checked={s.agent.auto_clean.when_disk_low} onchange={save} label="When disk low" /></div>
    <div class="setting"><div><strong>Daily report</strong><p class="muted small">Roz is samay din ki chhoti report.</p></div>
      <div class="row"><Toggle checked={s.agent.daily_summary !== null} onchange={(v) => { s!.agent.daily_summary = v ? "21:00" : null; save(); }} label="Daily report" />
        {#if s.agent.daily_summary !== null}<input type="time" bind:value={s.agent.daily_summary} onchange={save} />{/if}</div></div>
    <div class="setting"><div><strong>Weekly report</strong><p class="muted small">Hafte bhar ka hisaab.</p></div>
      <div class="row wrap"><Toggle checked={s.agent.weekly_summary !== null} onchange={(v) => { s!.agent.weekly_summary = v ? { weekday: 6, time: "20:00" } : null; save(); }} label="Weekly report" />
        {#if s.agent.weekly_summary}<select bind:value={s.agent.weekly_summary.weekday} onchange={save}>{#each DAYS as d, i}<option value={i}>{d}</option>{/each}</select>
          <input type="time" bind:value={s.agent.weekly_summary.time} onchange={save} />{/if}</div></div>
    <div class="setting"><div><strong>Smart alerts</strong><p class="muted small">Sirf lambi chalne wali dikkat par: RAM 15 min se bhari, disk lagbhag full, nayi startup app, suraksha band.</p></div>
      <Toggle bind:checked={s.agent.smart_alerts} onchange={save} label="Smart alerts" /></div>
  </section>

  <section class="card">
    <div class="card-title"><Icon name="inbox" /><h2>Notifications</h2></div>
    <div class="setting"><div><strong>Notifications dikhayein</strong></div><Toggle bind:checked={s.agent.notify.enabled} onchange={save} label="Notifications" /></div>
    <div class="setting"><div><strong>Awaaz (sound)</strong><p class="muted small">Ijazat maangte waqt halki si awaaz.</p></div><Toggle bind:checked={s.agent.notify.sound} onchange={save} label="Sound" /></div>
    <div class="setting"><div><strong>Din mein zyada se zyada</strong><p class="muted small">Alerts ki limit (reports aur ijazat isme nahi gine jate).</p></div>
      <input type="number" min="0" max="20" bind:value={s.agent.notify.max_per_day} onchange={save} style="width:80px" /></div>
    <div class="setting"><div><strong>Shaanti ka samay</strong><p class="muted small">Is beech pop-up nahi; messages inbox mein milenge.</p></div>
      <div class="row"><input type="time" bind:value={s.agent.notify.quiet_from} onchange={save} /><span class="muted">se</span><input type="time" bind:value={s.agent.notify.quiet_to} onchange={save} /></div></div>
  </section>

  <section class="card">
    <div class="card-title"><Icon name="bird" /><h2>Offline AI</h2><span class="spacer"></span>
      {#if ai}<span class="muted small">{bytes(ai.install.disk_bytes)} jagah · RAM {bytes(ai.ram_total)}{ai.has_gpu ? " · graphics card mila" : ""}</span>{/if}</div>
    <p class="muted small" style="margin-bottom:12px">Model sirf zarurat par kuch seconds ke liye load hota hai (summary, file check ya chat), phir band. Sab kuch isi PC par; internet sirf ek baar download ke liye.</p>
    {#if ai}
      <div class="models">
        {#each ai.models as m}
          {@const active = ai.enabled && ai.tier === m.tier && m.installed}
          <div class="model" class:active>
            <div class="row"><strong>{TIER_LABEL[m.tier]}</strong>
              {#if ai.recommended === m.tier}<span class="chip teal">Aapke PC ke liye</span>{/if}
              {#if active}<span class="chip good">Chalu</span>{/if}</div>
            <span class="mname">{m.name}</span>
            <p class="muted small">{m.description}</p>
            <span class="small">{bytes(m.size)} download · {m.license}</span>
            {#if !m.fits}<span class="small warn">Is PC ki RAM ke liye bhari ho sakta hai</span>{/if}
            {#if downloading && progress?.tier === m.tier}
              <div class="bar"><span style="width:{progress.total ? (progress.done / progress.total) * 100 : 3}%"></span></div>
              <div class="row"><span class="small muted">{progress.stage === "runtime" ? "AI engine" : "Model"}: {bytes(progress.done)} / {bytes(progress.total)}</span><span class="spacer"></span>
                <button class="btn ghost small" onclick={() => api.aiInstall(m.tier, true)}>Roko</button></div>
            {:else if m.installed}
              <div class="row">{#if !active}<button class="btn small primary" onclick={() => useModel(m.tier)}>Use karein</button>{/if}
                <button class="btn small ghost danger" onclick={() => remove(m.tier)}><Icon name="trash" size={15} />Hatao</button></div>
            {:else}
              <button class="btn small" class:primary={ai.recommended === m.tier} onclick={() => install(m.tier)} disabled={!!downloading}><Icon name="download" size={15} />Download</button>
            {/if}
          </div>
        {/each}
      </div>
      <div class="setting"><div><strong>AI chalu</strong></div><Toggle bind:checked={s.ai.enabled} onchange={save} label="AI" disabled={!ai.install.installed_tiers.length} /></div>
      <div class="setting"><div><strong>Shaq wali files AI se jaanchwayein</strong><p class="muted small">Auto safai mein jin files ki pehchaan pakki na ho, AI dekhe. AI kisi file ko "risky" bata de toh aapse poocha jayega.</p></div>
        <Toggle bind:checked={s.ai.review_files} onchange={save} label="Review files" /></div>
      <div class="setting"><div><strong>Reports AI likhe</strong><p class="muted small">Daily/weekly report ka friendly hissa AI likhega; numbers hamesha app ke apne.</p></div>
        <Toggle bind:checked={s.ai.write_summaries} onchange={save} label="AI summaries" /></div>
      <div class="setting"><div><strong>Graphics card use karein</strong><p class="muted small">Ho toh jawab tez aate hain.</p></div><Toggle bind:checked={s.ai.use_gpu} onchange={save} label="GPU" /></div>
      <div class="setting"><div><strong>Chat ke baad model band karein</strong><p class="muted small">Itni der kuch na poochein toh RAM wapas.</p></div>
        <select bind:value={s.ai.unload_after_secs} onchange={save}><option value={60}>1 minute</option><option value={120}>2 minute</option><option value={300}>5 minute</option><option value={900}>15 minute</option></select></div>
    {/if}
  </section>

  <p class="muted small about">{ui.info?.name ?? "Sweeplark"} {ui.info?.version} · {ui.info?.tagline_hi} · Na ads, na account, na tracking. Open source (GPL-3.0).</p>
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
