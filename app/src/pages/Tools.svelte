<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { api } from "../lib/api";
  import { t, toast } from "../lib/state.svelte";
  import type { PowerPlan, ToolId } from "../lib/types";

  let plans = $state<PowerPlan[]>([]);
  let hw = $state<[string, string][]>([]);
  let minutes = $state(60);
  let busy = $state<string | null>(null);

  onMount(() => {
    api.powerPlans().then((p) => (plans = p ?? [])).catch(() => {});
    api.hardware().then((h) => (hw = h?.items ?? [])).catch(() => {});
  });

  async function run(id: ToolId) {
    busy = id;
    try { toast(await api.tool(id)); } catch (e) { toast(String(e), "warn"); }
    busy = null;
  }
  async function setPlan(guid: string | null) {
    try { await api.setPowerPlan(guid); plans = await api.powerPlans(); toast(t("Power plan changed", "Power plan badal gaya")); }
    catch (e) { toast(String(e), "warn"); }
  }
  async function shutdown(restart: boolean) {
    try { await api.shutdown(minutes, restart); toast(t(`PC will ${restart ? "restart" : "shut down"} in ${minutes} min`, `PC ${minutes} min mein ${restart ? "restart" : "band"} hoga`)); }
    catch (e) { toast(String(e), "warn"); }
  }

  const REPAIRS = $derived<[ToolId, string, string, string, boolean][]>([
    ["restart_explorer", "restart", t("Restart Explorer", "Explorer restart"), t("Fixes a stuck taskbar or Start menu and applies Explorer tweaks.", "Atka taskbar/Start theek, Explorer tweaks lagu."), false],
    ["rebuild_icon_cache", "file", t("Rebuild icon cache", "Icon cache dobara banao"), t("Fixes blank or wrong icons.", "Khaali ya galat icons theek."), false],
    ["sfc", "shield", t("Repair system files (SFC)", "System files repair (SFC)"), t("Checks and repairs Windows system files.", "Windows ki system files check aur repair."), true],
    ["dism", "shield", t("Repair Windows image (DISM)", "Windows image repair (DISM)"), t("Fixes the component store SFC depends on. Needs internet.", "SFC jis par chalta hai use theek kare. Internet chahiye."), true],
    ["disk_check", "disk", t("Check disk for errors", "Disk errors check"), t("Online scan of drive C: for file-system errors.", "C: drive ke file-system errors ka scan."), true],
    ["reset_windows_update", "download", t("Fix Windows Update", "Windows Update theek karo"), t("Resets update components when updates keep failing.", "Updates baar-baar fail hon to components reset."), true],
  ]);
  const NETWORK = $derived<[ToolId, string, string, boolean][]>([
    ["flush_dns", t("Flush DNS cache", "DNS cache saaf"), t("Fixes sites that won't load after a change.", "Badlav ke baad na khulne wali sites theek."), false],
    ["reset_network", t("Reset network", "Network reset"), t("Resets Winsock and TCP/IP. Restart needed.", "Winsock aur TCP/IP reset. Restart chahiye."), true],
  ]);
  const WINTOOLS = $derived<[string, string][]>([
    ["task_manager", "Task Manager"], ["device_manager", "Device Manager"], ["services", "Services"], ["event_viewer", "Event Viewer"],
    ["disk_management", "Disk Management"], ["resource_monitor", "Resource Monitor"], ["system_info", "System Information"], ["disk_cleanup", "Disk Cleanup"],
    ["apps", t("Installed apps", "Installed apps")], ["windows_update", "Windows Update"], ["storage_sense", "Storage Sense"], ["environment", t("Environment variables", "Environment variables")],
  ]);
</script>

<header class="head"><div><h1>{t("Tools", "Tools")}</h1><p class="muted">{t("Repairs, network fixes, power plans, a shutdown timer and your PC's hardware details.", "Repair, network fix, power plans, shutdown timer aur PC ki hardware details.")}</p></div></header>

<section class="grid">
  <div class="card">
    <div class="card-title"><Icon name="shield" /><h3>{t("Repair", "Repair")}</h3></div>
    <div class="list">{#each REPAIRS as [id, icon, name, desc, admin]}
      <div class="item"><span class="ic"><Icon name={icon} size={16} /></span><div class="col grow" style="gap:1px"><strong>{name}</strong><span class="muted small">{desc}</span></div>
        {#if admin}<span class="chip"><Icon name="lock" size={11} />Admin</span>{/if}
        <button class="btn small" disabled={busy === id} onclick={() => run(id)}>{busy === id ? "…" : t("Run", "Chalao")}</button></div>
    {/each}</div>
  </div>
  <div class="col" style="gap:14px">
    <div class="card">
      <div class="card-title"><Icon name="download" /><h3>{t("Network", "Network")}</h3></div>
      <div class="list">{#each NETWORK as [id, name, desc, admin]}
        <div class="item"><div class="col grow" style="gap:1px"><strong>{name}</strong><span class="muted small">{desc}</span></div>
          {#if admin}<span class="chip"><Icon name="lock" size={11} />Admin</span>{/if}
          <button class="btn small" disabled={busy === id} onclick={() => run(id)}>{t("Run", "Chalao")}</button></div>
      {/each}</div>
    </div>
    <div class="card">
      <div class="card-title"><Icon name="battery" /><h3>{t("Power plan", "Power plan")}</h3></div>
      <div class="plans">{#each plans as p}<button class="plan" class:on={p.active} onclick={() => setPlan(p.guid)}>{p.name}{#if p.active}<Icon name="check" size={14} />{/if}</button>{/each}
        {#if !plans.some((p) => p.name.toLowerCase().includes("ultimate"))}<button class="plan add" onclick={() => setPlan(null)}>+ Ultimate Performance</button>{/if}</div>
    </div>
    <div class="card">
      <div class="card-title"><Icon name="history" /><h3>{t("Shutdown timer", "Shutdown timer")}</h3></div>
      <div class="row wrap"><input type="number" min="1" max="1440" bind:value={minutes} style="width:90px" /><span class="muted">min</span>
        <button class="btn small" onclick={() => shutdown(false)}>{t("Shut down", "Band karo")}</button>
        <button class="btn small" onclick={() => shutdown(true)}>{t("Restart", "Restart")}</button>
        <button class="btn small ghost" onclick={() => run("cancel_shutdown")}>{t("Cancel timer", "Timer cancel")}</button></div>
    </div>
  </div>
</section>

<section class="grid" style="margin-top:14px">
  <div class="card">
    <div class="card-title"><Icon name="cpu" /><h3>{t("This PC", "Yeh PC")}</h3></div>
    {#each hw as [k, v]}<div class="kv"><span class="muted">{k}</span><span class="selectable">{v}</span></div>{:else}<div class="row"><div class="spin"></div></div>{/each}
  </div>
  <div class="card">
    <div class="card-title"><Icon name="settings" /><h3>{t("Windows tools", "Windows tools")}</h3></div>
    <div class="wt">{#each WINTOOLS as [id, name]}<button class="btn small" onclick={() => api.windowsTool(id).catch((e) => toast(String(e), "warn"))}>{name}</button>{/each}</div>
  </div>
</section>

<style>
  .head { margin-bottom: 18px; }
  .grid { display: grid; grid-template-columns: 1.2fr 1fr; gap: 14px; align-items: start; }
  .grow { flex: 1; min-width: 0; }
  .ic { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 9px; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .plans { display: flex; flex-wrap: wrap; gap: 8px; }
  .plan { display: inline-flex; align-items: center; gap: 6px; padding: 8px 14px; border-radius: 10px; border: 1px solid var(--line); background: var(--surface-2); cursor: pointer; color: var(--ink); }
  .plan.on { border-color: var(--teal); background: var(--teal-soft); color: var(--teal-text); font-weight: 600; }
  .plan.add { border-style: dashed; color: var(--slate); }
  .kv { display: flex; justify-content: space-between; gap: 16px; padding: 7px 0; border-top: 1px solid var(--line); font-size: 13px; }
  .kv:first-of-type { border-top: 0; }
  .kv span:last-child { text-align: right; }
  .wt { display: flex; flex-wrap: wrap; gap: 8px; }
  @media (max-width: 1000px) { .grid { grid-template-columns: 1fr; } }
</style>
