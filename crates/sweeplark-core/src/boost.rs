//! Boost: free RAM and calm down background CPU/GPU hogs, in one click, with the user
//! choosing what to do. Uses documented Windows mechanisms only:
//! * RAM: trim working sets of background apps (`EmptyWorkingSet`) and, as admin, purge the
//!   standby list (what Sysinternals RAMMap does). Windows pages memory back in when needed.
//! * CPU/GPU: Windows Efficiency mode (EcoQoS + idle priority) for background apps.
//!
//! Never touched: the app you were using (the window behind Sweeplark when you click Boost),
//! its child processes, Windows itself, Sweeplark and its own WebView, apps on the user's
//! "never" list, and a built-in list of apps that break when slowed down (voice chat,
//! streaming, anti-cheat, security, drivers). Every Efficiency-mode change records the old
//! priority, so Boost can be undone.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::health::is_system_process;
use crate::monitor::{Monitor, ProcInfo};
use crate::platform::system;
use crate::revert::RevertStep;

/// Apps that misbehave when slowed or trimmed: voice/video calls (audio crackles), streaming
/// and recording, music players, anti-cheat (games can kick you), security software, and
/// hardware helpers (RGB, audio drivers, GPU services), remote desktop.
const PROTECTED: &[&str] = &[
    // calls and chat with voice
    "discord", "discordptb", "discordcanary", "teams", "ms-teams", "zoom", "skype", "teamspeak", "ts3client_win64",
    "ts3client_win32", "mumble", "slack", "webex", "ciscocollabhost", "whatsapp", "telegram", "signal",
    // streaming, recording, music
    "obs64", "obs32", "streamlabs obs", "xsplit.core", "nvidia share", "nvidia overlay", "spotify", "vlc", "foobar2000",
    "musicbee", "aimp", "itunes", "wmplayer", "audacity", "voicemeeter", "voicemeeterpro", "voicemeeter8x64",
    // anti-cheat and launchers' game services
    "easyanticheat", "easyanticheat_eos", "beservice", "beservice_x64", "battleye", "vgc", "vgtray", "faceit",
    "faceitclient", "faceitservice", "eaanticheat.gameservice", "gameguard", "xigncode", "ricochet",
    // security
    "mbam", "mbamservice", "avp", "avastsvc", "avastui", "avgsvc", "avgui", "bdservicehost", "bdagent", "ekrn", "egui",
    "nortonsecurity", "ns", "mcshield", "mfemms", "sophoshealth", "savservice",
    // drivers and hardware helpers
    "nvcontainer", "nvdisplay.container", "nvidia web helper", "amdrsserv", "radeonsoftware", "atieclxx", "atiesrxx",
    "rtkaudservice64", "rtkauduservice64", "nahimicsvc64", "nahimicsvc32", "igfxem", "igfxcuiservice", "lghub",
    "lghub_agent", "icue", "razer synapse 3", "razercentralservice", "steelseriesgg", "armourycrate.service",
    "asus_framework", "msi.centralserver", "dragoncenter",
    // remote desktop
    "anydesk", "teamviewer", "teamviewer_service", "rustdesk", "parsecd", "sunshine",
    // Windows shell parts that sysinfo lists as normal apps
    "searchhost", "startmenuexperiencehost", "shellexperiencehost", "applicationframehost", "textinputhost",
    "systemsettings", "lockapp", "securityhealthsystray", "smartscreen", "taskmgr", "wmiprvse", "phoneexperiencehost",
];

pub fn is_protected(name: &str) -> bool {
    let n = name.to_lowercase();
    let n = n.trim_end_matches(".exe");
    PROTECTED.contains(&n)
}

#[derive(Debug, Clone, Serialize)]
pub struct BoostApp {
    pub name: String,
    pub pids: Vec<u32>,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub gpu_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoostPlan {
    pub ram_used: u64,
    pub ram_total: u64,
    pub cpu_percent: f32,
    pub gpu_percent: Option<f32>,
    /// The app you were using; never touched.
    pub foreground: Option<String>,
    /// Background apps, heaviest RAM first.
    pub apps: Vec<BoostApp>,
    /// Running apps left out on purpose (protected list or your "never" list).
    pub protected: Vec<String>,
    pub can_purge_standby: bool,
    /// On battery: the power plan option is skipped to save the battery.
    pub on_battery: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BoostOptions {
    pub trim_ram: bool,
    pub purge_standby: bool,
    pub eco_cpu: bool,
    pub eco_gpu: bool,
    pub high_performance: bool,
    /// App names the user ticked. `None` = every background app that qualifies;
    /// `Some([])` = none (only the system-wide options run).
    #[serde(default)]
    pub apps: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcoChange {
    pub pid: u32,
    pub name: String,
    /// Priority class before Boost (restored on undo).
    pub priority: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BoostReport {
    pub ram_before: u64,
    pub ram_after: u64,
    pub trimmed: u32,
    pub standby_purged: bool,
    /// App names put into Efficiency mode.
    pub eco: Vec<String>,
    pub eco_changes: Vec<EcoChange>,
    pub power_plan: Option<String>,
    /// GUID of the plan that was active before (for undo), when Boost changed it.
    pub previous_plan: Option<String>,
    pub errors: Vec<String>,
}

impl BoostReport {
    /// Steps that put everything back (for the change ledger).
    pub fn revert_steps(&self) -> Vec<RevertStep> {
        let mut steps: Vec<RevertStep> = self
            .eco_changes
            .iter()
            .map(|c| RevertStep::ProcessPriority { pid: c.pid, name: c.name.clone(), priority: c.priority })
            .collect();
        if let Some(guid) = &self.previous_plan {
            steps.insert(0, RevertStep::PowerPlan { guid: guid.clone() });
        }
        steps
    }
}

const CPU_HOG: f32 = 3.0;
const GPU_HOG: f32 = 3.0;
/// Windows' IDLE_PRIORITY_CLASS: the process is already as low as Eco would make it.
const IDLE_PRIORITY: u32 = 0x40;

/// All processes started (directly or not) by one of `roots`.
fn descendants(procs: &[ProcInfo], roots: &[u32]) -> HashSet<u32> {
    let mut set: HashSet<u32> = roots.iter().copied().collect();
    loop {
        let before = set.len();
        for p in procs {
            if let Some(parent) = p.parent
                && set.contains(&parent)
                && p.pid != parent
            {
                set.insert(p.pid);
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

struct Grouped {
    apps: Vec<BoostApp>,
    foreground: Option<String>,
    protected: Vec<String>,
}

fn base_name(name: &str) -> String {
    name.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
}

fn group(procs: &[ProcInfo], gpu: &HashMap<u32, f32>, keep_pid: Option<u32>, never: &[String]) -> Grouped {
    let own = descendants(procs, &[std::process::id()]);
    let fg_name = keep_pid.and_then(|k| procs.iter().find(|p| p.pid == k)).map(|p| base_name(&p.name));
    let fg_tree = keep_pid.map(|k| descendants(procs, &[k])).unwrap_or_default();
    let never_lc: Vec<String> = never.iter().map(|n| n.trim().trim_end_matches(".exe").to_lowercase()).collect();
    let mut protected: HashSet<String> = HashSet::new();
    let mut map: HashMap<String, BoostApp> = HashMap::new();
    for p in procs {
        let name = base_name(&p.name);
        let key = name.to_lowercase();
        if p.name.is_empty() || own.contains(&p.pid) || fg_tree.contains(&p.pid) || is_system_process(&p.name) {
            continue;
        }
        if fg_name.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&name)) {
            continue;
        }
        if is_protected(&p.name) || never_lc.contains(&key) {
            if p.memory > 30 * 1024 * 1024 || p.cpu > 1.0 {
                protected.insert(name);
            }
            continue;
        }
        let a = map.entry(key).or_insert_with(|| BoostApp {
            name: name.clone(),
            pids: Vec::new(),
            memory_bytes: 0,
            cpu_percent: 0.0,
            gpu_percent: 0.0,
        });
        a.pids.push(p.pid);
        a.memory_bytes += p.memory;
        a.cpu_percent += p.cpu;
        a.gpu_percent += gpu.get(&p.pid).copied().unwrap_or(0.0);
    }
    let mut apps: Vec<BoostApp> =
        map.into_values().filter(|a| a.memory_bytes > 30 * 1024 * 1024 || a.cpu_percent > 1.0 || a.gpu_percent > 1.0).collect();
    apps.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes));
    apps.truncate(40);
    let mut protected: Vec<String> = protected.into_iter().collect();
    protected.sort_by_key(|n| n.to_lowercase());
    Grouped { apps, foreground: fg_name, protected }
}

fn on_battery() -> bool {
    system::battery().is_some_and(|b| b.on_ac_power == Some(false))
}

/// RAM guard: when memory stays nearly full, trim background apps (RAM only; no Efficiency
/// mode, no power plan). The app in front is left alone. Returns bytes freed.
pub fn ram_guard(monitor: &mut Monitor, never: &[String]) -> (u64, u32) {
    let opts = BoostOptions { trim_ram: true, ..Default::default() };
    let r = apply(monitor, &opts, never, None);
    (r.ram_before.saturating_sub(r.ram_after), r.trimmed)
}

pub fn plan(monitor: &mut Monitor, never: &[String]) -> BoostPlan {
    let stats = monitor.sample();
    let procs = monitor.raw_processes();
    let gpu: HashMap<u32, f32> = monitor.gpu_per_process().into_iter().collect();
    let g = group(&procs, &gpu, system::window_behind_own(), never);
    BoostPlan {
        ram_used: stats.ram_used,
        ram_total: stats.ram_total,
        cpu_percent: stats.cpu_percent,
        gpu_percent: stats.gpu_percent,
        foreground: g.foreground,
        apps: g.apps,
        protected: g.protected,
        can_purge_standby: system::is_admin(),
        on_battery: on_battery(),
    }
}

/// Run Boost. `keep_pid` is the app to leave alone (`None` = find the window behind Sweeplark).
pub fn apply(monitor: &mut Monitor, opts: &BoostOptions, never: &[String], keep_pid: Option<u32>) -> BoostReport {
    let mut rep = BoostReport { ram_before: monitor.sample().ram_used, ..Default::default() };
    let procs = monitor.raw_processes();
    let gpu: HashMap<u32, f32> =
        if opts.eco_gpu { monitor.gpu_per_process().into_iter().collect() } else { HashMap::new() };
    let keep = keep_pid.or_else(system::window_behind_own);
    let g = group(&procs, &gpu, keep, never);
    let chosen = |a: &BoostApp| match &opts.apps {
        None => true,
        Some(list) => list.iter().any(|n| n.eq_ignore_ascii_case(&a.name)),
    };
    for a in g.apps.iter().filter(|a| chosen(a)) {
        if opts.trim_ram {
            for pid in &a.pids {
                if system::trim_working_set(*pid) {
                    rep.trimmed += 1;
                }
            }
        }
        let hog = (opts.eco_cpu && a.cpu_percent >= CPU_HOG) || (opts.eco_gpu && a.gpu_percent >= GPU_HOG);
        if hog {
            let mut any = false;
            for pid in &a.pids {
                let before = system::priority_class(*pid);
                if before == Some(IDLE_PRIORITY) {
                    continue; // already calm (maybe by an earlier Boost): nothing to record
                }
                if system::set_efficiency_mode(*pid, true).is_ok() {
                    any = true;
                    rep.eco_changes.push(EcoChange {
                        pid: *pid,
                        name: format!("{}.exe", a.name),
                        priority: before.unwrap_or(0x20), // NORMAL_PRIORITY_CLASS
                    });
                }
            }
            if any {
                rep.eco.push(a.name.clone());
            }
        }
    }
    if opts.purge_standby {
        // Deep clean: first write changed pages out (they become standby), then drop standby.
        let _ = system::flush_modified_list();
        match system::purge_standby_list() {
            Ok(()) => rep.standby_purged = true,
            Err(e) => rep.errors.push(e.to_string()),
        }
    }
    if opts.high_performance {
        if on_battery() {
            rep.errors.push("On battery: power plan left as it is to save the battery.".into());
        } else {
            match crate::platform::ops::fastest_plan() {
                Ok(p) if p.active => rep.power_plan = Some(p.name),
                Ok(p) => {
                    let before = crate::platform::ops::active_power_plan();
                    match crate::platform::ops::set_power_plan(&p.guid) {
                        Ok(()) => {
                            rep.power_plan = Some(p.name);
                            rep.previous_plan = before;
                        }
                        Err(e) => rep.errors.push(e.to_string()),
                    }
                }
                Err(e) => rep.errors.push(e.to_string()),
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(400));
    rep.ram_after = monitor.sample().ram_used;
    crate::log::info(&format!(
        "boost: trimmed {} processes, eco {:?}, plan {:?}, errors {}",
        rep.trimmed,
        rep.eco,
        rep.power_plan,
        rep.errors.len()
    ));
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: Option<u32>, name: &str, mb: u64, cpu: f32) -> ProcInfo {
        ProcInfo { pid, parent, name: name.into(), cpu, memory: mb * 1024 * 1024 }
    }

    #[test]
    fn keeps_foreground_own_tree_and_protected_apps_out() {
        let own = std::process::id();
        let procs = vec![
            p(own, Some(1), "sweeplark.exe", 80, 1.0),
            p(9001, Some(own), "msedgewebview2.exe", 200, 5.0), // our own WebView
            p(9002, Some(77), "msedgewebview2.exe", 150, 0.5),  // another app's WebView
            p(500, Some(1), "game.exe", 4000, 60.0),
            p(501, Some(500), "crashhandler.exe", 40, 0.0),
            p(600, Some(1), "Discord.exe", 400, 8.0),
            p(700, Some(1), "chrome.exe", 900, 6.0),
            p(701, Some(700), "chrome.exe", 300, 2.0),
            p(800, Some(1), "Backup.exe", 120, 4.0),
            p(4, None, "System", 1, 1.0),
        ];
        let g = group(&procs, &HashMap::new(), Some(500), &["backup".into()]);
        let names: Vec<&str> = g.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(g.foreground.as_deref(), Some("game"));
        assert!(names.contains(&"chrome"));
        assert!(!names.contains(&"game") && !names.contains(&"crashhandler"));
        assert!(!names.contains(&"Discord"), "voice chat is protected");
        assert!(!names.contains(&"Backup"), "user's never list");
        let webview = g.apps.iter().find(|a| a.name == "msedgewebview2").unwrap();
        assert_eq!(webview.pids, vec![9002], "our own WebView is never touched");
        assert!(g.protected.iter().any(|n| n == "Discord") && g.protected.iter().any(|n| n == "Backup"));
        let chrome = g.apps.iter().find(|a| a.name == "chrome").unwrap();
        assert_eq!(chrome.pids.len(), 2);
    }

    #[test]
    fn empty_selection_means_none() {
        let o: BoostOptions = serde_json::from_str(r#"{"trim_ram":true,"purge_standby":false,"eco_cpu":true,"eco_gpu":false,"high_performance":false,"apps":[]}"#).unwrap();
        assert_eq!(o.apps, Some(vec![]));
        let o: BoostOptions = serde_json::from_str(r#"{"trim_ram":true,"purge_standby":false,"eco_cpu":true,"eco_gpu":false,"high_performance":false}"#).unwrap();
        assert!(o.apps.is_none());
    }

    #[test]
    fn report_reverts_power_plan_and_priorities() {
        let r = BoostReport {
            eco_changes: vec![EcoChange { pid: 5, name: "x.exe".into(), priority: 0x8000 }],
            previous_plan: Some("abc".into()),
            ..Default::default()
        };
        let steps = r.revert_steps();
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0], RevertStep::PowerPlan { .. }));
    }
}
