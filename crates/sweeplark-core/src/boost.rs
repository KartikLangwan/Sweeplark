//! Boost: free RAM and calm down background CPU/GPU hogs, in one click, with the user
//! choosing what to do. Uses documented Windows mechanisms only:
//! * RAM: trim working sets of background apps (`EmptyWorkingSet`) and, as admin, purge the
//!   standby list (what Sysinternals RAMMap does). Windows pages memory back in when needed.
//! * CPU/GPU: Windows Efficiency mode (EcoQoS + idle priority) for background apps.
//!
//! The app in front (what you are using right now), Windows processes and Sweeplark itself
//! are never touched.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::health::is_system_process;
use crate::monitor::Monitor;
use crate::platform::system;

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
    /// The app in front; never touched.
    pub foreground: Option<String>,
    /// Background apps, heaviest RAM first.
    pub apps: Vec<BoostApp>,
    pub can_purge_standby: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BoostOptions {
    pub trim_ram: bool,
    pub purge_standby: bool,
    pub eco_cpu: bool,
    pub eco_gpu: bool,
    pub high_performance: bool,
    /// App names the user ticked (empty = all background apps that qualify).
    #[serde(default)]
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BoostReport {
    pub ram_before: u64,
    pub ram_after: u64,
    pub trimmed: u32,
    pub standby_purged: bool,
    pub eco: Vec<String>,
    pub power_plan: Option<String>,
    pub errors: Vec<String>,
}

const CPU_HOG: f32 = 3.0;
const GPU_HOG: f32 = 3.0;

fn group(monitor: &mut Monitor) -> (Vec<BoostApp>, Option<String>) {
    let fg_pid = system::foreground_pid();
    let own = std::process::id();
    let gpu: HashMap<u32, f32> = monitor.gpu_per_process().into_iter().collect();
    let mut fg_name = None;
    let mut map: HashMap<String, BoostApp> = HashMap::new();
    for (pid, name, cpu, mem) in monitor.raw_processes() {
        if Some(pid) == fg_pid {
            fg_name = Some(name.trim_end_matches(".exe").to_string());
        }
        if pid == own || name.is_empty() || is_system_process(&name) {
            continue;
        }
        let a = map.entry(name.to_lowercase()).or_insert_with(|| BoostApp {
            name: name.trim_end_matches(".exe").to_string(),
            pids: Vec::new(),
            memory_bytes: 0,
            cpu_percent: 0.0,
            gpu_percent: 0.0,
        });
        a.pids.push(pid);
        a.memory_bytes += mem;
        a.cpu_percent += cpu;
        a.gpu_percent += gpu.get(&pid).copied().unwrap_or(0.0);
    }
    // Drop the whole foreground app (all its processes).
    if let Some(fg) = &fg_name {
        map.remove(&fg.to_lowercase());
        map.remove(&format!("{}.exe", fg.to_lowercase()));
    }
    let mut apps: Vec<BoostApp> = map.into_values().filter(|a| a.memory_bytes > 30 * 1024 * 1024 || a.cpu_percent > 1.0 || a.gpu_percent > 1.0).collect();
    apps.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes));
    apps.truncate(40);
    (apps, fg_name)
}

pub fn plan(monitor: &mut Monitor) -> BoostPlan {
    let stats = monitor.sample();
    let (apps, foreground) = group(monitor);
    BoostPlan {
        ram_used: stats.ram_used,
        ram_total: stats.ram_total,
        cpu_percent: stats.cpu_percent,
        gpu_percent: stats.gpu_percent,
        foreground,
        apps,
        can_purge_standby: system::is_admin(),
    }
}

pub fn apply(monitor: &mut Monitor, opts: &BoostOptions) -> BoostReport {
    let mut rep = BoostReport { ram_before: monitor.sample().ram_used, ..Default::default() };
    let (apps, _) = group(monitor);
    let chosen = |a: &BoostApp| opts.apps.is_empty() || opts.apps.iter().any(|n| n.eq_ignore_ascii_case(&a.name));
    for a in apps.iter().filter(|a| chosen(a)) {
        if opts.trim_ram {
            for pid in &a.pids {
                if system::trim_working_set(*pid) {
                    rep.trimmed += 1;
                }
            }
        }
        let hog = (opts.eco_cpu && a.cpu_percent >= CPU_HOG) || (opts.eco_gpu && a.gpu_percent >= GPU_HOG);
        if hog {
            let ok = a.pids.iter().filter(|p| system::set_efficiency_mode(**p, true).is_ok()).count();
            if ok > 0 {
                rep.eco.push(a.name.clone());
            }
        }
    }
    if opts.purge_standby {
        match system::purge_standby_list() {
            Ok(()) => rep.standby_purged = true,
            Err(e) => rep.errors.push(e.to_string()),
        }
    }
    if opts.high_performance {
        let plans = crate::platform::ops::power_plans();
        let target = plans
            .iter()
            .find(|p| p.name.to_lowercase().contains("ultimate"))
            .or_else(|| plans.iter().find(|p| p.guid == "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"));
        match target {
            Some(p) if !p.active => match crate::platform::ops::set_power_plan(&p.guid) {
                Ok(()) => rep.power_plan = Some(p.name.clone()),
                Err(e) => rep.errors.push(e.to_string()),
            },
            Some(p) => rep.power_plan = Some(p.name.clone()),
            None => rep.errors.push("High performance power plan not found".into()),
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(400));
    rep.ram_after = monitor.sample().ram_used;
    rep
}
