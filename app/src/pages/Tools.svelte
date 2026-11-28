<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import { api } from "../lib/api";
  import { t, toast } from "../lib/state.svelte";
  import type { PowerPlan, ToolId } from "../lib/types";

  let plans = $state<PowerPlan[]>([]);
  let hw = $state<[string, string][]>([]);
  let minutes = $state(60);
  let busy = $state<string | null>(null);
  let ask = $state<{ id: ToolId; title: string; text: string } | null>(null);

  // Tools that interrupt something get a short "are you sure" first.
  const CONFIRM = $derived<Partial<Record<ToolId, [string, string]>>>({
    restart_explorer: [t("Restart Explorer?", "Explorer restart karein?"), t("The taskbar and open folder windows close for a few seconds. Apps keep running.", "Taskbar aur khuli folder windows kuch seconds ke liye band hongi. Apps chalti rahengi.")],
    rebuild_icon_cache: [t("Rebuild the icon cache?", "Icon cache dobara banayein?"), t("Explorer restarts and icons may look blank for a moment.", "Explorer restart hoga, icons thodi der khaali dikh sakte hain.")],
    reset_network: [t("Reset the network?", "Network reset karein?"), t("The internet drops for a moment, and Wi-Fi and VPN settings may need setting up again. Restart the PC afterwards.", "Internet thodi der ke liye jayega, Wi-Fi/VPN settings dobara karni pad sakti hain. Baad mein PC restart karein.")],
    reset_windows_update: [t("Reset Windows Update?", "Windows Update reset karein?"), t("Update services stop for a minute and downloaded updates are downloaded again.", "Update services ek minute ruk jayengi aur downloaded updates dobara download honge.")],
    component_cleanup: [t("Remove old Windows update files?", "Purani Windows update files hatayein?"), t("Frees space in WinSxS. Installed updates can no longer be uninstalled afterwards. Takes several minutes.", "WinSxS mein jagah khaali hoti hai. Iske baad lage hue updates uninstall nahi ho payenge. Kuch minute lagenge.")],
  });

  function start(id: ToolId) {
    const c = CONFIRM[id];
    if (c) ask = { id, title: c[0], text: c[1] };
    else run(id);
  }

  onMount(() => {
    api.powerPlans().then((p) => (plans = p ?? [])).catch(() => {});
    api.hardware().then((h) => (hw = h?.items ?? [])).catch(() => {});
  });

  async function run(id: ToolId) {
    ask = null;
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
    ["component_cleanup", "broom", t("Clean old Windows update files", "Purani Windows update files saaf"), t("DISM component cleanup: often frees several GB in WinSxS.", "DISM component cleanup: WinSxS mein aksar kai GB khaali."), true],
    ["restore_point", "undo", t("Create a restore point", "Restore point banayein"), t("A safety copy of Windows settings you can go back to.", "Windows settings ki safety copy, jis par wapas ja sakte hain."), true],
  ]);
  const NETWORK = $derived<[ToolId, string, string, string, boolean][]>([
    ["flush_dns", "sparkle", t("Flush DNS cache", "DNS cache saaf"), t("Fixes sites that won't load after a change.", "Badlav ke baad na khulne wali sites theek."), false],
    ["reset_network", "restart", t("Reset network", "Network reset"), t("Resets Winsock and TCP/IP. Restart needed.", "Winsock aur TCP/IP reset. Restart chahiye."), true],
  ]);
  const WINTOOLS = $derived<[string, string][]>([
    ["task_manager", "Task Manager"], ["device_manager", "Device Manager"], ["services", "Services"], ["event_viewer", "Event Viewer"],
    ["disk_management", "Disk Management"], ["resource_monitor", "Resource Monitor"], ["system_info", "System Information"], ["disk_cleanup", "Disk Cleanup"],
    ["apps", t("Installed apps", "Installed apps")], ["windows_update", "Windows Update"], ["storage_sense", "Storage Sense"], ["environment", t("Environment variables", "Environment variables")],
  ]);
</script>

<header class="head"><div><h1>Tools</h1><p class="muted">{t("Repairs, network fixes, power plans, a shutdown timer and your PC's hardware details.", "Repair, network fix, power plans, shutdown timer aur PC ki hardware details.")}</p></div></header>

<section class="group">
  <h2 class="group-title">{t("Fix problems", "Problem theek karo")}</h2>
  <div class="card">
    <div class="card-title"><Icon name="shield" /><h3>{t("Repair", "Repair")}</h3></div>
    <p class="muted small admin-note"><Icon name="lock" size={13} />{t("Tools marked Admin ask Windows for permission (a UAC prompt) and open in their own window. No need to restart Sweeplark.", "Admin wale tools Windows se ijazat (UAC) maangte hain aur apni window mein khulte hain. Sweeplark restart karne ki zarurat nahi.")}</p>
    <div class="tiles">{#each REPAIRS as [id, icon, name, desc, admin]}
      <div class="tile">
        <span class="ic"><Icon name={icon} size={16} /></span>
        <div class="col grow" style="gap:1px"><strong>{name}</strong><span class="muted small">{desc}</span></div>
        <div class="tile-end">
          {#if admin}<span class="chip"><Icon name="lock" size={11} />{t("Admin", "Admin")}</span>{/if}
          <button class="btn small" disabled={busy === id} onclick={() => start(id)}>{busy === id ? "…" : t("Run", "Chalao")}</button>
        </div>
      </div>
    {/each}</div>
  </div>

  <div class="card">
    <div class="card-title"><Icon name="download" /><h3>{t("Network", "Network")}</h3></div>
    <div class="tiles">{#each NETWORK as [id, icon, name, desc, admin]}
      <div class="tile">
        <span class="ic"><Icon name={icon} size={16} /></span>
        <div class="col grow" style="gap:1px"><strong>{name}</strong><span class="muted small">{desc}</span></div>
        <div class="tile-end">
          {#if admin}<span class="chip"><Icon name="lock" size={11} />{t("Admin", "Admin")}</span>{/if}
          <button class="btn small" disabled={busy === id} onclick={() => start(id)}>{t("Run", "Chalao")}</button>
        </div>
      </div>
    {/each}</div>
  </div>
</section>

<section class="group">
  <h2 class="group-title">{t("Power", "Power")}</h2>
  <div class="grid">
    <div class="card">
      <div class="card-title"><Icon name="battery" /><h3>{t("Power plan", "Power plan")}</h3></div>
      <div class="plans">{#each plans as p}<button class="plan" class:on={p.active} onclick={() => setPlan(p.guid)}>{p.name}{#if p.active}<Icon name="check" size={14} />{/if}</button>{/each}
        {#if !plans.some((p) => p.name.toLowerCase().includes("ultimate"))}<button class="plan add" onclick={() => setPlan(null)}>+ Ultimate Performance</button>{/if}</div>
    </div>
    <div class="card">
      <div class="card-title"><Icon name="history" /><h3>{t("Shutdown timer", "Shutdown timer")}</h3></div>
      <div class="timer">
        <label class="field"><span class="muted small">{t("Minutes from now", "Abhi se kitne minute")}</span>
          <input type="number" min="1" max="1440" bind:value={minutes} style="width:110px" /></label>
        <div class="row wrap">
          <button class="btn" onclick={() => shutdown(false)}>{t("Shut down", "Band karo")}</button>
          <button class="btn" onclick={() => shutdown(true)}>{t("Restart", "Restart")}</button>
          <button class="btn ghost" onclick={() => run("cancel_shutdown")}>{t("Cancel timer", "Timer cancel")}</button>
        </div>
      </div>
    </div>
  </div>
</section>

<section class="group">
  <h2 class="group-title">{t("This PC", "Yeh PC")}</h2>
  <div class="grid">
    <div class="card">
      <div class="card-title"><Icon name="cpu" /><h3>{t("Hardware", "Hardware")}</h3></div>
      {#each hw as [k, v]}<div class="kv"><span class="muted">{k}</span><span class="selectable">{v}</span></div>{:else}<div class="row"><div class="spin"></div></div>{/each}
    </div>
    <div class="card">
      <div class="card-title"><Icon name="settings" /><h3>{t("Windows tools", "Windows tools")}</h3></div>
      <p class="muted small" style="margin-bottom:12px">{t("Opens the built-in Windows tool.", "Windows ka built-in tool khulega.")}</p>
      <div class="wt">{#each WINTOOLS as [id, name]}<button class="btn small" onclick={() => api.windowsTool(id).catch((e) => toast(String(e), "warn"))}>{name}</button>{/each}</div>
    </div>
  </div>
</section>

<Modal open={ask !== null} title={ask?.title ?? ""}>
  <p class="muted">{ask?.text}</p>
  {#snippet footer()}<button class="btn" onclick={() => (ask = null)}>{t("Cancel", "Ruko")}</button><button class="btn primary" onclick={() => ask && run(ask.id)}>{t("Run", "Chalao")}</button>{/snippet}
</Modal>

<style>
  .admin-note { display: flex; gap: 6px; align-items: center; margin: -4px 0 10px; }
  .head { margin-bottom: 20px; }
  .head p { margin-top: 4px; }
  .group { margin-bottom: 26px; }
  .group-title { font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--slate); font-weight: 600; margin: 0 2px 10px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; align-items: stretch; }
  .grow { flex: 1; min-width: 0; }
  .ic { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 9px; background: var(--teal-soft); color: var(--teal-text); flex: none; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 10px; }
  .tile { display: flex; align-items: center; gap: 12px; padding: 12px 14px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface-2); min-width: 0; }
  .tile-end { display: flex; align-items: center; gap: 8px; flex: none; }
  .plans { display: flex; flex-wrap: wrap; gap: 8px; }
  .plan { display: inline-flex; align-items: center; gap: 6px; padding: 8px 14px; border-radius: 10px; border: 1px solid var(--line); background: var(--surface-2); cursor: pointer; color: var(--ink); }
  .plan.on { border-color: var(--teal); background: var(--teal-soft); color: var(--teal-text); font-weight: 600; }
  .plan.add { border-style: dashed; color: var(--slate); }
  .timer { display: flex; flex-direction: column; gap: 14px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .kv { display: flex; justify-content: space-between; gap: 16px; padding: 8px 0; border-top: 1px solid var(--line); font-size: 13px; }
  .kv:first-of-type { border-top: 0; padding-top: 0; }
  .kv span:last-child { text-align: right; }
  .wt { display: flex; flex-wrap: wrap; gap: 8px; }
  @media (max-width: 1000px) { .grid { grid-template-columns: 1fr; } }
</style>
