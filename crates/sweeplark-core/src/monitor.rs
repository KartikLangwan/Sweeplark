//! Live monitor: CPU (total and per core), RAM, GPU usage/VRAM/temperature, network and
//! disk throughput, and a process list. Keep one `Monitor` alive and call `sample()` every
//! second or two; each call is cheap.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;
use sysinfo::{Components, Networks, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::health::is_system_process;

#[derive(Debug, Clone, Default, Serialize)]
pub struct LiveStats {
    pub cpu_percent: f32,
    pub per_core: Vec<f32>,
    pub cpu_mhz: u64,
    /// Real CPU sensor, when Windows exposes one.
    pub cpu_temp: Option<f32>,
    /// ACPI thermal zone (motherboard / chassis), not the CPU itself.
    pub board_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub gpu_name: Option<String>,
    pub gpu_percent: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub net_down_bps: u64,
    pub net_up_bps: u64,
    pub disk_read_bps: Option<u64>,
    pub disk_write_bps: Option<u64>,
    pub processes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessGroup {
    pub name: String,
    pub pids: Vec<u32>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub system: bool,
}

/// One process for Boost: id, parent, name, CPU (% of the whole CPU) and RAM.
#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

pub struct Monitor {
    sys: System,
    networks: Option<Networks>,
    components: Option<Components>,
    last: Instant,
    /// When the process table was last refreshed (CPU % needs two refreshes apart).
    last_procs: Option<Instant>,
    gpu: gpu::GpuReader,
    gpu_name: Option<String>,
    /// CPU sensor reading and when it was taken (asked at most every 5 s; WMI is not free).
    cpu_temp: Option<(Instant, Option<f32>)>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new().expect("system info");
        sys.refresh_cpu_all();
        sys.refresh_memory();
        // Name of the main graphics card for every vendor (NVML only knows NVIDIA).
        let gpu_name = sysinfo::Gpus::new_with_refreshed_list().ok().and_then(|list| {
            let names: Vec<String> = list
                .iter()
                .map(|g| [g.vendor(), g.model()].into_iter().flatten().collect::<Vec<_>>().join(" ").trim().to_string())
                .filter(|n| !n.is_empty() && !n.to_lowercase().contains("basic display"))
                .collect();
            names.iter().find(|n| !n.to_lowercase().contains("intel")).or(names.first()).cloned()
        });
        Self {
            sys,
            networks: Networks::new_with_refreshed_list().ok(),
            components: Components::new_with_refreshed_list().ok(),
            last: Instant::now(),
            last_procs: None,
            gpu: gpu::GpuReader::new(),
            gpu_name,
            cpu_temp: None,
        }
    }

    pub fn sample(&mut self) -> LiveStats {
        let secs = self.last.elapsed().as_secs_f64().max(0.2);
        self.last = Instant::now();
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        let (mut down, mut up) = (0u64, 0u64);
        if let Some(n) = self.networks.as_mut() {
            n.refresh(true);
            for data in n.values() {
                down += data.received();
                up += data.transmitted();
            }
        }
        let mut cpu_temp = None;
        if cfg!(windows) {
            // On Windows the generic sensors are ACPI thermal zones (motherboard), not the CPU.
            let fresh = self.cpu_temp.is_some_and(|(t, _)| t.elapsed() < std::time::Duration::from_secs(5));
            if !fresh {
                self.cpu_temp = Some((Instant::now(), crate::platform::system::cpu_temp_sensor()));
            }
            cpu_temp = self.cpu_temp.and_then(|(_, v)| v);
        } else if let Some(c) = self.components.as_mut() {
            c.refresh(false);
            cpu_temp = c
                .iter()
                .filter(|c| {
                    let l = c.label().to_lowercase();
                    ["cpu", "package", "core", "tctl", "tdie"].iter().any(|w| l.contains(w))
                })
                .filter_map(|c| c.temperature())
                .filter(|t| *t > 0.0 && *t < 130.0)
                .reduce(f32::max);
        }
        let g = self.gpu.read();
        LiveStats {
            cpu_percent: self.sys.global_cpu_usage(),
            per_core: self.sys.cpus().iter().map(|c| c.usage()).collect(),
            cpu_mhz: self.sys.cpus().first().map(|c| c.frequency()).unwrap_or(0),
            cpu_temp,
            board_temp: g.thermal_zone,
            ram_used: self.sys.used_memory(),
            ram_total: self.sys.total_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            gpu_name: g.name.or_else(|| self.gpu_name.clone()),
            gpu_percent: g.usage,
            gpu_temp: g.temp,
            vram_used: g.vram_used,
            vram_total: g.vram_total,
            net_down_bps: (down as f64 / secs) as u64,
            net_up_bps: (up as f64 / secs) as u64,
            disk_read_bps: g.disk_read,
            disk_write_bps: g.disk_write,
            processes: self.sys.processes().len(),
        }
    }

    /// Refresh the process table so CPU % is meaningful: the first refresh (or one after a
    /// long pause) only sets a baseline, so a second one follows shortly after.
    fn refresh_procs(&mut self) {
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
        let stale = self.last_procs.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(10));
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        if stale {
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(400)));
            self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        }
        self.last_procs = Some(Instant::now());
    }

    /// Running apps grouped by name (all Chrome processes together), heaviest first.
    pub fn processes(&mut self) -> Vec<ProcessGroup> {
        self.refresh_procs();
        let cores = self.sys.cpus().len().max(1) as f32;
        let mut map: HashMap<String, ProcessGroup> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            let name = p.name().to_string_lossy().into_owned();
            if name.is_empty() || name.eq_ignore_ascii_case("System Idle Process") {
                continue;
            }
            let g = map.entry(name.to_lowercase()).or_insert_with(|| ProcessGroup {
                system: is_system_process(&name),
                name: name.trim_end_matches(".exe").to_string(),
                pids: Vec::new(),
                cpu_percent: 0.0,
                memory_bytes: 0,
            });
            g.pids.push(pid.as_u32());
            g.cpu_percent += p.cpu_usage() / cores;
            g.memory_bytes += p.memory();
        }
        let mut list: Vec<ProcessGroup> = map.into_values().collect();
        list.sort_by_key(|g| std::cmp::Reverse(g.memory_bytes));
        list
    }

    /// GPU use per process id (Windows only; empty elsewhere).
    pub fn gpu_per_process(&mut self) -> Vec<(u32, f32)> {
        self.gpu.per_process()
    }

    /// Fresh process table for Boost.
    pub fn raw_processes(&mut self) -> Vec<ProcInfo> {
        self.refresh_procs();
        let cores = self.sys.cpus().len().max(1) as f32;
        self.sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcInfo {
                pid: pid.as_u32(),
                parent: p.parent().map(|x| x.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage() / cores,
                memory: p.memory(),
            })
            .collect()
    }

    /// Ids of Sweeplark and everything it started (its WebView, the AI engine).
    fn own_tree(&self) -> std::collections::HashSet<u32> {
        let mut set = std::collections::HashSet::from([std::process::id()]);
        loop {
            let before = set.len();
            for (pid, p) in self.sys.processes() {
                if p.parent().is_some_and(|pp| set.contains(&pp.as_u32())) {
                    set.insert(pid.as_u32());
                }
            }
            if set.len() == before {
                return set;
            }
        }
    }

    /// End every process of an app. Windows and Sweeplark's own processes are refused.
    pub fn end_app(&mut self, name: &str) -> crate::Result<u32> {
        if is_system_process(name) {
            return Err(crate::Error::Other(format!("{name} is part of Windows and can't be ended")));
        }
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let own = self.own_tree();
        let want = name.to_lowercase();
        let mut n = 0;
        for (pid, p) in self.sys.processes() {
            if own.contains(&pid.as_u32()) {
                continue;
            }
            let pname = p.name().to_string_lossy().to_lowercase();
            if (pname == want || pname.trim_end_matches(".exe") == want) && p.kill() {
                n += 1;
            }
        }
        crate::log::info(&format!("ended {name}: {n} processes"));
        if n == 0 {
            return Err(crate::Error::Other(format!("could not end {name} (it may need administrator rights)")));
        }
        Ok(n)
    }
}

#[derive(Debug, Default)]
struct GpuSample {
    name: Option<String>,
    usage: Option<f32>,
    temp: Option<f32>,
    vram_used: Option<u64>,
    vram_total: Option<u64>,
    thermal_zone: Option<f32>,
    disk_read: Option<u64>,
    disk_write: Option<u64>,
}

#[cfg(not(windows))]
mod gpu {
    use super::GpuSample;
    pub struct GpuReader;
    impl GpuReader {
        pub fn new() -> Self {
            GpuReader
        }
        pub fn read(&mut self) -> GpuSample {
            GpuSample::default()
        }
        pub fn per_process(&mut self) -> Vec<(u32, f32)> {
            Vec::new()
        }
    }
}

#[cfg(windows)]
mod gpu {
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
        PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::{HSTRING, PCWSTR};

    use super::GpuSample;

    /// Windows performance counters (work for every GPU vendor, no admin needed).
    struct Pdh {
        query: PDH_HQUERY,
        gpu_3d: PDH_HCOUNTER,
        gpu_all: PDH_HCOUNTER,
        vram: PDH_HCOUNTER,
        disk_read: PDH_HCOUNTER,
        disk_write: PDH_HCOUNTER,
        thermal: PDH_HCOUNTER,
    }

    impl Pdh {
        fn new() -> Option<Self> {
            unsafe {
                let mut query = PDH_HQUERY::default();
                if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                    return None;
                }
                let add = |path: &str| {
                    let mut c = PDH_HCOUNTER::default();
                    let ok = PdhAddEnglishCounterW(query, &HSTRING::from(path), 0, &mut c) == 0;
                    if ok { c } else { PDH_HCOUNTER::default() }
                };
                let pdh = Pdh {
                    query,
                    gpu_3d: add(r"\GPU Engine(*engtype_3D)\Utilization Percentage"),
                    gpu_all: add(r"\GPU Engine(*)\Utilization Percentage"),
                    vram: add(r"\GPU Adapter Memory(*)\Dedicated Usage"),
                    disk_read: add(r"\PhysicalDisk(_Total)\Disk Read Bytes/sec"),
                    disk_write: add(r"\PhysicalDisk(_Total)\Disk Write Bytes/sec"),
                    thermal: add(r"\Thermal Zone Information(*)\Temperature"),
                };
                PdhCollectQueryData(query);
                Some(pdh)
            }
        }

        fn values(&self, counter: PDH_HCOUNTER) -> Option<Vec<f64>> {
            self.named(counter).map(|v| v.into_iter().map(|(_, x)| x).collect())
        }

        /// All instance (name, value) pairs of a (wildcard) counter.
        fn named(&self, counter: PDH_HCOUNTER) -> Option<Vec<(String, f64)>> {
            if counter.is_invalid() {
                return None;
            }
            unsafe {
                let (mut size, mut count) = (0u32, 0u32);
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None) != PDH_MORE_DATA {
                    return None;
                }
                let items = (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()).max(1);
                let mut buf = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items];
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(buf.as_mut_ptr())) != 0 {
                    return None;
                }
                Some(
                    buf.iter()
                        .take(count as usize)
                        .filter(|i| i.FmtValue.CStatus == 0)
                        .map(|i| (i.szName.to_string().unwrap_or_default(), i.FmtValue.Anonymous.doubleValue))
                        .collect(),
                )
            }
        }
    }

    // PDH query and counter handles may be used from any thread; the Monitor that owns
    // them sits behind a Mutex, so only one thread uses them at a time.
    unsafe impl Send for Pdh {}

    impl Drop for Pdh {
        fn drop(&mut self) {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }

    pub struct GpuReader {
        pdh: Option<Pdh>,
        nvml: Option<nvml_wrapper::Nvml>,
    }

    impl GpuReader {
        pub fn new() -> Self {
            // NVML ships with the NVIDIA driver; on other PCs this simply fails.
            Self { pdh: Pdh::new(), nvml: nvml_wrapper::Nvml::init().ok() }
        }

        /// GPU use per process id (all engines), from instance names like `pid_1234_luid_..._engtype_3D`.
        pub fn per_process(&mut self) -> Vec<(u32, f32)> {
            let Some(p) = &self.pdh else { return Vec::new() };
            // Utilization is a rate between two collections: give it half a second, not the
            // few milliseconds since the last read.
            unsafe {
                PdhCollectQueryData(p.query);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            unsafe {
                PdhCollectQueryData(p.query);
            }
            let mut map: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
            for (name, v) in p.named(p.gpu_all).unwrap_or_default() {
                if let Some(pid) = name.strip_prefix("pid_").and_then(|r| r.split('_').next()).and_then(|n| n.parse().ok()) {
                    *map.entry(pid).or_default() += v;
                }
            }
            map.into_iter().filter(|(_, v)| *v > 0.5).map(|(k, v)| (k, v.min(100.0) as f32)).collect()
        }

        pub fn read(&mut self) -> GpuSample {
            let mut s = GpuSample::default();
            if let Some(p) = &self.pdh {
                unsafe {
                    PdhCollectQueryData(p.query);
                }
                // Sum 3D use per graphics adapter (instance names carry "luid_..."), then take the
                // busiest adapter: adding an integrated and a dedicated GPU together is meaningless.
                s.usage = p.named(p.gpu_3d).map(|items| {
                    let mut per: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
                    for (name, v) in items {
                        let luid = name.split("_phys").next().and_then(|a| a.split("luid_").nth(1)).unwrap_or("").to_string();
                        *per.entry(luid).or_default() += v;
                    }
                    per.values().copied().fold(0.0, f64::max).min(100.0) as f32
                });
                s.vram_used = p.values(p.vram).map(|v| v.iter().sum::<f64>() as u64);
                s.disk_read = p.values(p.disk_read).and_then(|v| v.first().copied()).map(|v| v as u64);
                s.disk_write = p.values(p.disk_write).and_then(|v| v.first().copied()).map(|v| v as u64);
                // Thermal zones report Kelvin.
                s.thermal_zone = p
                    .values(p.thermal)
                    .and_then(|v| v.into_iter().map(|k| k - 273.15).filter(|c| *c > 1.0 && *c < 130.0).reduce(f64::max))
                    .map(|c| c as f32);
            }
            if let Some(nvml) = &self.nvml
                && let Ok(dev) = nvml.device_by_index(0)
            {
                s.name = dev.name().ok();
                s.temp = dev.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu).ok().map(|t| t as f32);
                if let Ok(u) = dev.utilization_rates() {
                    s.usage = Some(u.gpu as f32);
                }
                if let Ok(m) = dev.memory_info() {
                    s.vram_used = Some(m.used);
                    s.vram_total = Some(m.total);
                }
            }
            s
        }
    }
}
