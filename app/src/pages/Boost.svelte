<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Ring from "../components/Ring.svelte";
  import Toggle from "../components/Toggle.svelte";
  import JobProgress from "../components/JobProgress.svelte";
  import AdminBanner from "../components/AdminBanner.svelte";
  import { api } from "../lib/api";
  import { bytes } from "../lib/format";
  import { results, runJob, running } from "../lib/jobs.svelte";
  import { saveSettings, t, toast, ui } from "../lib/state.svelte";
  import type { Settings } from "../lib/types";

  const plan = $derived(results.boostPlan);
  const report = $derived(results.boost);
  const busy = $derived(running("boost"));
  let loading = $state(false);
  let opts = $state({ trim_ram: true, purge_standby: false, eco_cpu: true, eco_gpu: true, high_performance: false });
  results.sel.boost ??= {};
  const picked = results.sel.boost;

  onMount(() => { load(); });
  async function load() {
    loading = true;
    try {
      const p = await api.boostPlan();
      for (const a of p.apps) if (!(a.name in picked)) picked[a.name] = true;
      results.boostPlan = p;
      // Deep clean is offered only after the user ticks it: dropping the file cache makes
      // apps load files from disk again.
    } catch (e) { toast(String(e), "warn"); }
    loading = false;
  }

  const ram = $derived(plan ? (plan.ram_used / plan.ram_total) * 100 : 0);
  const chosen = $derived(plan ? plan.apps.filter((a) => picked[a.name]) : []);
  const anything = $derived(opts.trim_ram || opts.purge_standby || opts.eco_cpu || opts.eco_gpu || opts.high_performance);
  const freed = $derived(report ? Math.max(0, report.ram_before - report.ram_after) : 0);

  async function boost() {
    // Always send the list: an empty list means "no apps", never "all apps".
    const r = await runJob("boost", t("Boosting", "Boost ho raha hai"), "boost", () => api.boostApply({ ...opts, apps: chosen.map((a) => a.name) }));
    if (!r) return;
    results.boost = r;
    toast(t(`Boost done: ${bytes(Math.max(0, r.ram_before - r.ram_after))} RAM freed`, `Boost ho gaya: ${bytes(Math.max(0, r.ram_before - r.ram_after))} RAM khaali`));
    for (const e of r.errors) toast(e, "warn");
    load();
  }
  async function restore() {
    try {
      const ok = await api.boostRestore();
      toast(ok ? t("Apps are back to normal", "Apps normal ho gayi") : t("Nothing to put back", "Wapas karne ko kuch nahi"), ok ? "good" : "info");
      results.boost = null;
      load();
    } catch (e) { toast(String(e), "warn"); }
  }
  async function setBoost(patch: Partial<Settings["boost"]>) {
    if (!ui.settings) return;
    const s = { ...$state.snapshot(ui.settings) as Settings, boost: { ...ui.settings.boost, ...patch } };
    try { await saveSettings(s); } catch (e) { toast(String(e), "warn"); }
  }
  async function never(name: string) {
    if (!ui.settings) return;
    await setBoost({ never: [...ui.settings.boost.never, name] });
    toast(t(`${name} will never be touched by Boost`, `${name} ko Boost kabhi nahi chhuega`));
    load();
  }
  const allPicked = $derived(plan ? plan.apps.every((a) => picked[a.name]) : false);
  function toggleAll() {
    const v = !allPicked;
    for (const a of plan?.apps ?? []) picked[a.name] = v;
  }

  const OPTIONS = $derived<[keyof typeof opts, string, string, string, boolean][]>([
    ["trim_ram", "ram", t("Free RAM from background apps", "Background apps ki RAM khaali karo"), t("Windows moves their unused memory out of RAM. They get it back when used again.", "Windows unki unused memory RAM se hata deta hai. Zarurat par wapas mil jaati hai."), false],
    ["purge_standby", "ram", t("Deep RAM clean (standby + modified memory)", "Deep RAM safai (standby + modified memory)"), t("Writes pending changes to disk and drops Windows' file cache from RAM, like RAMMap. Needs administrator.", "Pending data disk par likh kar Windows ka file cache RAM se hatata hai (RAMMap jaisa). Admin chahiye."), !plan?.can_purge_standby],
    ["eco_cpu", "cpu", t("Calm CPU-heavy background apps", "CPU khane wali background apps ko shaant karo"), t("Puts apps using CPU in the background into Windows Efficiency mode. Undo any time.", "Background mein CPU le rahi apps ko Windows Efficiency mode mein daalo. Kabhi bhi undo."), false],
    ["eco_gpu", "pulse", t("Calm GPU-heavy background apps", "GPU khane wali background apps ko shaant karo"), t("Same for apps using the graphics card in the background, so your game or app gets the GPU.", "GPU use kar rahi background apps ke liye bhi, taaki aapke game ko GPU mile."), false],
    ["high_performance", "battery", t("Fastest power plan", "Sabse tez power plan"), plan?.on_battery ? t("Skipped on battery to save power.", "Battery par bijli bachane ke liye chhoda jayega.") : t("Switches to Ultimate/High performance. Undo puts your old plan back.", "Ultimate/High performance par switch. Undo purana plan wapas laata hai."), false],
  ]);
</script>

<header class="head"><div><h1>Boost</h1>
  <p class="muted">{t("Free RAM and calm background CPU and GPU use in one click. The app you were using, voice chat, anti-cheat and Windows are never touched.", "Ek click mein RAM khaali aur background CPU/GPU shaant. Aapki chalti app, voice chat, anti-cheat aur Windows ko kabhi nahi chhuenge.")}</p></div></header>

<JobProgress id="boost" hint={t("Freeing memory…", "Memory khaali kar raha hoon…")} />

{#if plan}
  <section class="top">
    <div class="card gauge"><Ring value={ram} size={118} label="{Math.round(ram)}%" sub="RAM" tone={ram > 90 ? "red" : ram > 80 ? "amber" : "teal"} />
      <span class="muted small">{bytes(plan.ram_used)} / {bytes(plan.ram_total)}</span></div>
    <div class="card gauge"><Ring value={plan.cpu_percent} size={118} label="{Math.round(plan.cpu_percent)}%" sub="CPU" tone={plan.cpu_percent > 85 ? "amber" : "teal"} /></div>
    <div class="card gauge"><Ring value={plan.gpu_percent ?? 0} size={118} label={plan.gpu_percent != null ? `${Math.round(plan.gpu_percent)}%` : "–"} sub="GPU" /></div>
    <div class="card go">
      {#if report}
        <h2>{t(`${bytes(freed)} RAM freed`, `${bytes(freed)} RAM khaali`)}</h2>
        <p class="muted small">{t(`${report.trimmed} processes trimmed`, `${report.trimmed} processes trim`)}{report.standby_purged ? t(" · deep clean done", " · deep safai hui") : ""}{report.eco.length ? ` · Eco: ${report.eco.slice(0, 4).join(", ")}${report.eco.length > 4 ? "…" : ""}` : ""}{report.power_plan ? ` · ${report.power_plan}` : ""}</p>
      {:else}
        <h2>{t("Ready to boost", "Boost ke liye taiyaar")}</h2>
        <p class="muted small">{plan.foreground ? t(`Leaving ${plan.foreground} (the app you're using) alone.`, `${plan.foreground} (aapki chalti app) ko nahi chhuenge.`) : ""}</p>
      {/if}
      <div class="row wrap">
        <button class="btn primary big" disabled={busy || !anything} onclick={boost}><Icon name="rocket" />{t("Boost now", "Abhi Boost karo")}</button>
        {#if report?.ledger_id}<button class="btn" onclick={restore}><Icon name="undo" size={16} />{t("Put back", "Wapas normal")}</button>{/if}
      </div>
    </div>
  </section>

  <section class="two">
    <div class="col" style="gap:14px">
      <div class="card">
        <div class="card-title"><Icon name="sparkle" /><h3>{t("What to do", "Kya karna hai")}</h3></div>
        <div class="list">
          {#each OPTIONS as [key, icon, name, desc, disabled]}
            <label class="item opt" class:dim={disabled}>
              <input type="checkbox" bind:checked={opts[key]} {disabled} />
              <span class="ic"><Icon name={icon} size={15} /></span>
              <div class="col grow" style="gap:1px"><strong>{name}</strong><span class="muted small">{desc}</span></div>
            </label>
          {/each}
        </div>
        {#if !plan.can_purge_standby}<div style="margin-top:10px"><AdminBanner text={t("Deep RAM clean needs administrator rights. Restart as admin and come straight back here.", "Deep RAM safai ke liye admin chahiye. Admin mode mein kholein, seedha yahin wapas aayenge.")} /></div>{/if}
      </div>
      {#if ui.settings}
        <div class="card">
          <div class="card-title"><Icon name="game" /><h3>{t("Automatic", "Apne aap")}</h3></div>
          <div class="setting"><div><strong>{t("Game mode", "Game mode")}</strong><p class="muted small">{t("When a game runs full screen, heavy background apps calm down, and go back to normal when it closes.", "Game full screen chale toh bhari background apps shaant; game band hote hi normal.")}</p></div>
            <Toggle controlled checked={ui.settings.boost.auto_game} onchange={(v) => setBoost({ auto_game: v })} label="Game mode" /></div>
          <div class="setting"><div><strong>{t("RAM guard", "RAM guard")}</strong><p class="muted small">{t(`When RAM stays above ${ui.settings.boost.ram_guard_percent}% for a minute, free RAM from background apps (at most every 10 minutes).`, `RAM ek minute tak ${ui.settings.boost.ram_guard_percent}% se upar rahe toh background apps ki RAM khaali (10 minute mein ek baar).`)}</p></div>
            <Toggle controlled checked={ui.settings.boost.ram_guard} onchange={(v) => setBoost({ ram_guard: v })} label="RAM guard" /></div>
          {#if ui.settings.boost.ram_guard}
            <div class="row small muted"><span>{t("Limit", "Seema")}</span><input type="range" min="75" max="95" step="1" value={ui.settings.boost.ram_guard_percent} onchange={(e) => setBoost({ ram_guard_percent: Number((e.target as HTMLInputElement).value) })} /><span class="num">{ui.settings.boost.ram_guard_percent}%</span></div>
          {/if}
        </div>
      {/if}
    </div>
    <div class="card">
      <div class="card-title"><Icon name="cpu" /><h3>{t("Background apps", "Background apps")}</h3><span class="spacer"></span>
        <button class="btn ghost small" onclick={toggleAll}>{allPicked ? t("None", "Koi nahi") : t("All", "Sab")}</button>
        <button class="btn ghost small" onclick={load} disabled={loading}><Icon name="restart" size={14} />{t("Refresh", "Refresh")}</button></div>
      <div class="list">
        <div class="item th"><span class="cb"></span><span class="grow">{t("App", "App")}</span><span class="w">RAM</span><span class="w">CPU</span><span class="w">GPU</span><span class="nv"></span></div>
        {#each plan.apps as a (a.name)}
          <label class="item">
            <input type="checkbox" bind:checked={picked[a.name]} class="cb" />
            <span class="grow ell"><strong>{a.name}</strong></span>
            <span class="w num">{bytes(a.memory_bytes)}</span>
            <span class="w num" class:hot={a.cpu_percent >= 3}>{a.cpu_percent.toFixed(1)}%</span>
            <span class="w num" class:hot={a.gpu_percent >= 3}>{a.gpu_percent ? `${a.gpu_percent.toFixed(1)}%` : "–"}</span>
            <button class="btn ghost small nv" title={t("Never touch this app", "Is app ko kabhi mat chhuo")} aria-label={t(`Never touch ${a.name}`, `${a.name} ko kabhi mat chhuo`)} onclick={(e) => { e.preventDefault(); never(a.name); }}><Icon name="lock" size={13} /></button>
          </label>
        {:else}<div class="empty">{t("No heavy background apps right now.", "Abhi koi bhari background app nahi.")}</div>{/each}
      </div>
      {#if plan.protected.length}
        <p class="muted small prot"><Icon name="shield" size={13} />{t("Left alone on purpose:", "Jaan-boojh kar nahi chhua:")} {plan.protected.join(", ")}</p>
      {/if}
    </div>
  </section>
  <p class="muted small foot">{t("RAM that Windows frees fills up again as apps need it: that's normal. Boost helps most when RAM is nearly full or a background app hogs CPU/GPU while you game or work. Everything Boost changes is in History and can be undone.", "Windows jo RAM khaali karta hai, apps ki zarurat par wapas bharti hai: yeh normal hai. Boost tab sabse kaam aata hai jab RAM lagbhag full ho ya koi background app game/kaam ke waqt CPU/GPU kha rahi ho. Boost ke saare badlav History mein hain aur undo ho sakte hain.")}</p>
{:else}
  <div class="row"><div class="spin"></div><span class="muted">{t("Looking at running apps…", "Chal rahi apps dekh raha hoon…")}</span></div>
{/if}

<style>
  .head { margin-bottom: 18px; }
  .top { display: grid; grid-template-columns: repeat(3, 160px) 1fr; gap: 14px; margin-bottom: 14px; }
  .gauge { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; }
  .go { display: flex; flex-direction: column; justify-content: center; gap: 8px; align-items: flex-start; }
  .two { display: grid; grid-template-columns: 1fr 1.2fr; gap: 14px; align-items: start; }
  .opt { cursor: pointer; }
  .dim { opacity: .55; }
  .ic { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 8px; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .grow { flex: 1; min-width: 0; }
  .ell { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .w { width: 66px; text-align: right; font-size: 13px; }
  .nv { width: 30px; height: 28px; padding: 0; flex: none; display: grid; place-items: center; }
  .cb { width: 17px; }
  .th { font-size: 12px; color: var(--slate); font-weight: 600; }
  .hot { color: var(--amber-text); font-weight: 600; }
  .prot { display: flex; gap: 6px; align-items: flex-start; margin-top: 10px; }
  .setting { display: flex; justify-content: space-between; gap: 14px; align-items: center; padding: 8px 0; border-top: 1px solid var(--line); }
  .setting:first-of-type { border-top: 0; }
  .foot { margin-top: 12px; text-align: center; }
  @media (max-width: 1050px) { .top { grid-template-columns: repeat(3, 1fr); } .go { grid-column: 1 / -1; } .two { grid-template-columns: 1fr; } }
</style>
