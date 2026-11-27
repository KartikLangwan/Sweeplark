<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Ring from "../components/Ring.svelte";
  import { api } from "../lib/api";
  import { bytes } from "../lib/format";
  import { t, toast, ui } from "../lib/state.svelte";
  import type { BoostPlan, BoostReport } from "../lib/types";

  let plan = $state<BoostPlan | null>(null);
  let report = $state<BoostReport | null>(null);
  let busy = $state(false);
  let opts = $state({ trim_ram: true, purge_standby: false, eco_cpu: true, eco_gpu: true, high_performance: false });
  let picked = $state<Record<string, boolean>>({});

  onMount(load);
  async function load() {
    plan = await api.boostPlan().catch(() => null);
    if (plan) picked = Object.fromEntries(plan.apps.map((a) => [a.name, true]));
  }

  const ram = $derived(plan ? (plan.ram_used / plan.ram_total) * 100 : 0);
  const chosen = $derived(plan ? plan.apps.filter((a) => picked[a.name]) : []);
  const anything = $derived(opts.trim_ram || opts.purge_standby || opts.eco_cpu || opts.eco_gpu || opts.high_performance);

  async function boost() {
    busy = true;
    report = null;
    try {
      report = await api.boostApply({ ...opts, apps: chosen.map((a) => a.name) });
      const freed = Math.max(0, report.ram_before - report.ram_after);
      toast(t(`Boost done: ${bytes(freed)} RAM freed`, `Boost ho gaya: ${bytes(freed)} RAM khaali`));
      for (const e of report.errors) toast(e, "warn");
      await load();
    } catch (e) { toast(String(e), "warn"); }
    busy = false;
  }

  const OPTIONS = $derived<[keyof typeof opts, string, string, string, boolean][]>([
    ["trim_ram", "ram", t("Free RAM from background apps", "Background apps ki RAM khaali karo"), t("Windows moves their unused memory out of RAM. They get it back when used again.", "Windows unki unused memory RAM se hata deta hai. Zarurat par wapas mil jaati hai."), false],
    ["purge_standby", "ram", t("Clear standby memory", "Standby memory saaf karo"), t("Drops cached file data Windows keeps in RAM (like RAMMap). Needs administrator.", "Windows ka RAM mein rakha cache hatao (RAMMap jaisa). Admin chahiye."), !plan?.can_purge_standby],
    ["eco_cpu", "cpu", t("Calm CPU-heavy background apps", "CPU khane wali background apps ko shaant karo"), t("Puts apps using CPU in the background into Windows Efficiency mode.", "Background mein CPU le rahi apps ko Windows Efficiency mode mein daalo."), false],
    ["eco_gpu", "pulse", t("Calm GPU-heavy background apps", "GPU khane wali background apps ko shaant karo"), t("Same for apps using the graphics card in the background, so your game or app gets the GPU.", "GPU use kar rahi background apps ke liye bhi, taaki aapke game ko GPU mile."), false],
    ["high_performance", "battery", t("High performance power plan", "High performance power plan"), t("Switches to High/Ultimate Performance. Uses more power.", "High/Ultimate Performance par switch. Bijli zyada lagegi."), false],
  ]);
</script>

<header class="head"><div><h1>Boost</h1>
  <p class="muted">{t("Free RAM and calm background CPU and GPU use in one click. You choose what to do; the app you are using is never touched.", "Ek click mein RAM khaali aur background CPU/GPU shaant. Kya karna hai aap chunein; jo app aap use kar rahe hain use kabhi nahi chhuenge.")}</p></div></header>

{#if plan}
  <section class="top">
    <div class="card gauge"><Ring value={ram} size={118} label="{Math.round(ram)}%" sub="RAM" tone={ram > 90 ? "red" : ram > 80 ? "amber" : "teal"} />
      <span class="muted small">{bytes(plan.ram_used)} / {bytes(plan.ram_total)}</span></div>
    <div class="card gauge"><Ring value={plan.cpu_percent} size={118} label="{Math.round(plan.cpu_percent)}%" sub="CPU" tone={plan.cpu_percent > 85 ? "amber" : "teal"} /></div>
    <div class="card gauge"><Ring value={plan.gpu_percent ?? 0} size={118} label={plan.gpu_percent != null ? `${Math.round(plan.gpu_percent)}%` : "–"} sub="GPU" /></div>
    <div class="card go">
      {#if report}
        <h2>{t(`${bytes(Math.max(0, report.ram_before - report.ram_after))} RAM freed`, `${bytes(Math.max(0, report.ram_before - report.ram_after))} RAM khaali`)}</h2>
        <p class="muted small">{t(`${report.trimmed} processes trimmed`, `${report.trimmed} processes trim`)}{report.standby_purged ? t(" · standby cleared", " · standby saaf") : ""}{report.eco.length ? t(` · Eco: ${report.eco.join(", ")}`, ` · Eco: ${report.eco.join(", ")}`) : ""}{report.power_plan ? ` · ${report.power_plan}` : ""}</p>
      {:else}
        <h2>{t("Ready to boost", "Boost ke liye taiyaar")}</h2>
        <p class="muted small">{plan.foreground ? t(`Keeping ${plan.foreground} (the app in front) untouched.`, `${plan.foreground} (saamne wali app) ko nahi chhuenge.`) : ""}</p>
      {/if}
      <button class="btn primary big" disabled={busy || !anything} onclick={boost}>
        {#if busy}<div class="spin" style="border-top-color:#fff"></div>{:else}<Icon name="rocket" />{/if}{t("Boost now", "Abhi Boost karo")}</button>
    </div>
  </section>

  <section class="two">
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
      {#if !plan.can_purge_standby}<button class="btn small" style="margin-top:8px" onclick={() => api.relaunchAdmin()}><Icon name="lock" size={14} />{t("Restart as admin for standby clearing", "Standby saaf karne ke liye admin mode")}</button>{/if}
    </div>
    <div class="card">
      <div class="card-title"><Icon name="cpu" /><h3>{t("Background apps", "Background apps")}</h3><span class="spacer"></span>
        <button class="btn ghost small" onclick={load}><Icon name="restart" size={14} />{t("Refresh", "Refresh")}</button></div>
      <div class="list">
        <div class="item th"><span class="cb"></span><span class="grow">{t("App", "App")}</span><span class="w">RAM</span><span class="w">CPU</span><span class="w">GPU</span></div>
        {#each plan.apps as a (a.name)}
          <label class="item">
            <input type="checkbox" bind:checked={picked[a.name]} class="cb" />
            <span class="grow"><strong>{a.name}</strong></span>
            <span class="w num">{bytes(a.memory_bytes)}</span>
            <span class="w num" class:hot={a.cpu_percent >= 3}>{a.cpu_percent.toFixed(1)}%</span>
            <span class="w num" class:hot={a.gpu_percent >= 3}>{a.gpu_percent ? `${a.gpu_percent.toFixed(1)}%` : "–"}</span>
          </label>
        {:else}<div class="empty">{t("No heavy background apps right now.", "Abhi koi bhari background app nahi.")}</div>{/each}
      </div>
    </div>
  </section>
  <p class="muted small foot">{t("RAM that Windows frees fills up again as apps need it: that's normal. Boost helps most when RAM is nearly full or a background app is hogging CPU/GPU while you game or work.", "Windows jo RAM khaali karta hai, apps ki zarurat par wapas bharti hai: yeh normal hai. Boost tab sabse kaam aata hai jab RAM lagbhag full ho ya koi background app game/kaam ke waqt CPU/GPU kha rahi ho.")}</p>
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
  .w { width: 72px; text-align: right; font-size: 13px; }
  .cb { width: 17px; }
  .th { font-size: 12px; color: var(--slate); font-weight: 600; }
  .hot { color: var(--amber-text); font-weight: 600; }
  .foot { margin-top: 12px; text-align: center; }
  @media (max-width: 1050px) { .top { grid-template-columns: repeat(3, 1fr); } .go { grid-column: 1 / -1; } .two { grid-template-columns: 1fr; } }
</style>
