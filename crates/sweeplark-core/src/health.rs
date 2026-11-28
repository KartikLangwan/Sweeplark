//! Health snapshot: CPU, RAM, disks, heavy apps, temperatures, battery, security.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sysinfo::{Components, DiskKind, Disks, Gpus, MINIMUM_CPU_UPDATE_INTERVAL, System};

use crate::platform::system::{self, BatteryStatus, PhysicalDiskHealth, SecurityStatus};

#[derive(Debug, Clone, Serialize)]
pub struct DiskInfo {
    pub mount: String,
    pub name: String,
    pub file_system: String,
    /// "SSD", "HDD" or "Unknown".
    pub kind: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub removable: bool,
    pub is_system: bool,
}

impl DiskInfo {
    pub fn used_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (1.0 - self.free_bytes as f64 / self.total_bytes as f64) * 100.0
    }
}

/// One app, with all its processes added together (Chrome runs many).
#[derive(Debug, Clone, Serialize)]
pub struct AppUsage {
    pub name: String,
    pub processes: u32,
    pub memory_bytes: u64,
    /// Share of the whole CPU, 0-100.
    pub cpu_percent: f32,
    /// Part of Windows or of this app: never suggested for closing.
    #[serde(default)]
    pub system: bool,
}

/// Windows processes (and our own AI engine) that must never be suggested for closing.
const SYSTEM_PROCESSES: &[&str] = &[
    "system", "registry", "memory compression", "memcompression", "secure system", "smss", "csrss", "wininit",
    "winlogon", "services", "lsass", "lsaiso", "svchost", "dwm", "explorer", "sihost", "fontdrvhost", "ctfmon",
    "msmpeng", "nissrv", "securityhealthservice", "audiodg", "spoolsv", "searchindexer", "runtimebroker",
    "taskhostw", "wudfhost", "conhost", "dllhost", "llama-server", "sweeplark", "kernel_task", "init", "systemd",
    "searchhost", "startmenuexperiencehost", "shellexperiencehost", "textinputhost", "lsm", "wmiprvse", "lockapp",
    "securityhealthsystray", "smartscreen", "sgrmbroker", "wlanext", "unsecapp", "dashost", "spoolsv",
];

pub fn is_system_process(name: &str) -> bool {
    let n = name.to_lowercase();
    let n = n.trim_end_matches(".exe");
    SYSTEM_PROCESSES.contains(&n)
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub memory_bytes: Option<u64>,
    /// Integrated graphics share the main RAM; dedicated cards help the AI run faster.
    pub dedicated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Temperature {
    pub label: String,
    pub celsius: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthSnapshot {
    pub taken_at: DateTime<Utc>,
    pub os: String,
    pub host_name: String,
    pub uptime_secs: u64,
    pub is_admin: bool,
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub cpu_percent: f32,
    pub memory_total: u64,
    pub memory_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub process_count: usize,
    pub disks: Vec<DiskInfo>,
    pub gpus: Vec<GpuInfo>,
    pub top_memory: Vec<AppUsage>,
    pub top_cpu: Vec<AppUsage>,
    pub temperatures: Vec<Temperature>,
    pub battery: Option<BatteryStatus>,
    pub security: Option<SecurityStatus>,
    pub physical_disks: Vec<PhysicalDiskHealth>,
}

impl HealthSnapshot {
    pub fn memory_percent(&self) -> f64 {
        if self.memory_total == 0 {
            return 0.0;
        }
        self.memory_used as f64 / self.memory_total as f64 * 100.0
    }

    pub fn has_dedicated_gpu(&self) -> bool {
        self.gpus.iter().any(|g| g.dedicated)
    }

    pub fn system_disk(&self) -> Option<&DiskInfo> {
        self.disks.iter().find(|d| d.is_system)
    }
}

fn system_mount() -> String {
    if cfg!(windows) {
        format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()))
    } else {
        "/".into()
    }
}

fn disk_kind(kind: DiskKind) -> &'static str {
    match kind {
        DiskKind::SSD => "SSD",
        DiskKind::HDD => "HDD",
        DiskKind::Unknown(_) => "Unknown",
    }
}

const TOP_N: usize = 8;

fn group_apps(sys: &System, cores: usize) -> Vec<AppUsage> {
    let mut map: HashMap<String, AppUsage> = HashMap::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().into_owned();
        if name.is_empty() || name.eq_ignore_ascii_case("System Idle Process") {
            continue;
        }
        let e = map.entry(name.to_lowercase()).or_insert_with(|| AppUsage {
            name: name.trim_end_matches(".exe").to_string(),
            processes: 0,
            memory_bytes: 0,
            cpu_percent: 0.0,
            system: is_system_process(&name),
        });
        e.processes += 1;
        e.memory_bytes += p.memory();
        e.cpu_percent += p.cpu_usage() / cores.max(1) as f32;
    }
    map.into_values().collect()
}

/// Take a snapshot. Takes about a quarter second (CPU usage needs two samples).
/// `include_slow` also queries security, disk health and battery (WMI, slower on Windows).
pub fn snapshot(include_slow: bool) -> HealthSnapshot {
    let mut sys = System::new_all().unwrap_or_else(|_| System::new().expect("system info"));
    std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(250)));
    sys.refresh_all();

    let cores = sys.cpus().len().max(1);
    let sys_mount = system_mount().to_lowercase();
    let disks = Disks::new_with_refreshed_list()
        .map(|list| {
            list.iter()
                .filter(|d| d.total_space() > 0)
                .map(|d| {
                    let mount = d.mount_point().display().to_string();
                    DiskInfo {
                        is_system: mount.to_lowercase() == sys_mount,
                        mount,
                        name: d.name().to_string_lossy().into_owned(),
                        file_system: d.file_system().to_string_lossy().into_owned(),
                        kind: disk_kind(d.kind()).into(),
                        total_bytes: d.total_space(),
                        free_bytes: d.available_space(),
                        removable: d.is_removable(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Our own AI engine is not something the user should be told to close.
    let apps: Vec<AppUsage> =
        group_apps(&sys, cores).into_iter().filter(|a| !a.name.eq_ignore_ascii_case("llama-server")).collect();
    let mut top_memory = apps.clone();
    top_memory.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes));
    top_memory.truncate(TOP_N);
    let mut top_cpu = apps;
    top_cpu.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
    top_cpu.truncate(TOP_N);

    let mut temperatures: Vec<Temperature> = Components::new_with_refreshed_list()
        .map(|c| {
            c.iter()
                .filter_map(|c| {
                    c.temperature().filter(|t| *t > 0.0 && *t < 150.0).map(|t| Temperature {
                        // On Windows these are ACPI zones on the motherboard, not the CPU.
                        label: if cfg!(windows) { format!("Motherboard sensor ({})", c.label()) } else { c.label().to_string() },
                        celsius: t,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(cpu) = system::cpu_temp_sensor() {
        temperatures.insert(0, Temperature { label: "CPU".into(), celsius: cpu });
    }

    let gpus = Gpus::new_with_refreshed_list()
        .map(|list| {
            list.iter()
                .map(|g| {
                    let name = [g.vendor(), g.model()].into_iter().flatten().collect::<Vec<_>>().join(" ");
                    let lower = name.to_lowercase();
                    let memory_bytes = g.total_memory();
                    let dedicated = (lower.contains("nvidia") || lower.contains("radeon rx") || lower.contains("arc a"))
                        || memory_bytes.is_some_and(|m| m >= 2 * 1024 * 1024 * 1024 && !lower.contains("intel"));
                    GpuInfo { name: name.trim().to_string(), memory_bytes, dedicated }
                })
                .filter(|g| !g.name.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let os = System::long_os_version().or_else(|_| System::name()).unwrap_or_else(|_| std::env::consts::OS.into());

    HealthSnapshot {
        taken_at: Utc::now(),
        os,
        host_name: System::host_name().unwrap_or_default(),
        uptime_secs: System::uptime().unwrap_or(0),
        is_admin: system::is_admin(),
        cpu_brand: sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        cpu_cores: cores,
        cpu_percent: sys.global_cpu_usage(),
        memory_total: sys.total_memory(),
        memory_used: sys.used_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
        process_count: sys.processes().len(),
        disks,
        gpus,
        top_memory,
        top_cpu,
        temperatures,
        battery: if include_slow { system::battery() } else { None },
        security: if include_slow { system::security_status() } else { None },
        physical_disks: if include_slow { system::physical_disks() } else { Vec::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_snapshot_has_basics() {
        let s = snapshot(false);
        assert!(s.memory_total > 0);
        assert!(s.cpu_cores > 0);
        assert!(s.memory_percent() > 0.0 && s.memory_percent() <= 100.0);
        assert!(s.process_count > 0);
    }
}
