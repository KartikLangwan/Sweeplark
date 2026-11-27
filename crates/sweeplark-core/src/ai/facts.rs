//! "Tools" for the assistant. Small models cannot call tools reliably, so the *app* runs
//! them and hands the model a short text of measured facts. Every tool is read-only.

use std::cell::OnceCell;

use serde::{Deserialize, Serialize};

use crate::advisor::{self, Finding};
use crate::cleaner::{PreviewStatus, ScanReport};
use crate::health::HealthSnapshot;
use crate::ledger::{ChangeRecord, LedgerSummary};
use crate::startup::{Advice, StartupItem};
use crate::util::format_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Specs,
    Performance,
    Storage,
    Startup,
    Security,
    Battery,
    Heat,
    History,
    Problems,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Specs,
        Tool::Performance,
        Tool::Storage,
        Tool::Startup,
        Tool::Security,
        Tool::Battery,
        Tool::Heat,
        Tool::History,
        Tool::Problems,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Specs => "specs",
            Tool::Performance => "performance",
            Tool::Storage => "storage",
            Tool::Startup => "startup",
            Tool::Security => "security",
            Tool::Battery => "battery",
            Tool::Heat => "heat",
            Tool::History => "history",
            Tool::Problems => "problems",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Tool::Specs => "PC hardware and Windows version",
            Tool::Performance => "RAM and CPU use, heavy apps, uptime",
            Tool::Storage => "drive space and junk files found",
            Tool::Startup => "apps that start with Windows",
            Tool::Security => "antivirus and firewall status",
            Tool::Battery => "battery charge and wear",
            Tool::Heat => "temperatures",
            Tool::History => "what the app cleaned or changed recently",
            Tool::Problems => "list of problems the app found",
        }
    }

    pub fn from_name(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.name() == s)
    }
}

/// Where facts come from. The real one reads the PC; tests use fixed data.
pub trait PcSource {
    fn health(&self) -> HealthSnapshot;
    /// Junk scan (slowest tool, only run when needed).
    fn scan(&self) -> Option<ScanReport>;
    fn startup(&self) -> Vec<StartupItem>;
    fn history(&self) -> Option<(LedgerSummary, Vec<ChangeRecord>)>;
}

/// Facts gathered lazily: each source is read at most once per question.
pub struct Facts<'a> {
    src: &'a dyn PcSource,
    pub lang: crate::Lang,
    health: OnceCell<HealthSnapshot>,
    scan: OnceCell<Option<ScanReport>>,
    startup: OnceCell<Vec<StartupItem>>,
    history: OnceCell<Option<(LedgerSummary, Vec<ChangeRecord>)>>,
}

fn gb(bytes: u64) -> String {
    format_bytes(bytes)
}

impl<'a> Facts<'a> {
    pub fn new(src: &'a dyn PcSource, lang: crate::Lang) -> Self {
        Self { src, lang, health: OnceCell::new(), scan: OnceCell::new(), startup: OnceCell::new(), history: OnceCell::new() }
    }

    pub fn health(&self) -> &HealthSnapshot {
        self.health.get_or_init(|| self.src.health())
    }

    pub fn scan(&self) -> Option<&ScanReport> {
        self.scan.get_or_init(|| self.src.scan()).as_ref()
    }

    pub fn startup(&self) -> &[StartupItem] {
        self.startup.get_or_init(|| self.src.startup())
    }

    fn history(&self) -> Option<&(LedgerSummary, Vec<ChangeRecord>)> {
        self.history.get_or_init(|| self.src.history()).as_ref()
    }

    /// Problems found, using whatever facts are already loaded (plus health and startup).
    pub fn findings(&self) -> Vec<Finding> {
        advisor::analyze(self.health(), self.scan.get().and_then(|s| s.as_ref()), Some(self.startup()), self.lang)
    }

    /// Run one tool and describe the result in a few short lines.
    pub fn run(&self, tool: Tool) -> String {
        match tool {
            Tool::Specs => {
                let h = self.health();
                let disk = h
                    .system_disk()
                    .map(|d| format!(", {} {} {} total", d.mount.trim_end_matches('\\'), d.kind, gb(d.total_bytes)))
                    .unwrap_or_default();
                let gpu = h.gpus.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(" + ");
                format!(
                    "{} | CPU {} ({} threads) | RAM {}{}{}",
                    h.os,
                    h.cpu_brand,
                    h.cpu_cores,
                    gb(h.memory_total),
                    disk,
                    if gpu.is_empty() { String::new() } else { format!(" | GPU {gpu}") }
                )
            }
            Tool::Performance => {
                let h = self.health();
                let apps = |list: &[crate::health::AppUsage], cpu: bool| {
                    list.iter()
                        .take(4)
                        .map(|a| {
                            if cpu {
                                format!("{} {:.0}%", a.name, a.cpu_percent)
                            } else {
                                format!("{} {}", a.name, gb(a.memory_bytes))
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                format!(
                    "RAM {:.0}% used ({} of {}). Most RAM: {}.\nCPU {:.0}% now. Most CPU: {}.\nRunning {} days without restart. {} processes.",
                    h.memory_percent(),
                    gb(h.memory_used),
                    gb(h.memory_total),
                    apps(&h.top_memory, false),
                    h.cpu_percent,
                    apps(&h.top_cpu, true),
                    h.uptime_secs / 86_400,
                    h.process_count
                )
            }
            Tool::Storage => {
                let h = self.health();
                let mut lines: Vec<String> = h
                    .disks
                    .iter()
                    .filter(|d| !d.removable)
                    .map(|d| {
                        format!(
                            "{} ({}): {} free of {} ({:.0}% full)",
                            d.mount.trim_end_matches('\\'),
                            d.kind,
                            gb(d.free_bytes),
                            gb(d.total_bytes),
                            d.used_percent()
                        )
                    })
                    .collect();
                if let Some(scan) = self.scan() {
                    let mut ready: Vec<_> =
                        scan.cleaners.iter().filter(|c| matches!(c.status, PreviewStatus::Ready)).collect();
                    ready.sort_by_key(|c| std::cmp::Reverse(c.bytes));
                    let top =
                        ready.iter().take(4).map(|c| format!("{} {}", c.name, gb(c.bytes))).collect::<Vec<_>>().join(", ");
                    lines.push(format!("Junk files found: {} ({top}).", gb(scan.total_bytes)));
                }
                lines.join("\n")
            }
            Tool::Startup => {
                let items = self.startup();
                let on: Vec<&StartupItem> = items.iter().filter(|i| i.enabled).collect();
                let can_off: Vec<&str> =
                    on.iter().filter(|i| i.advice == Advice::Disable).map(|i| i.name.as_str()).take(6).collect();
                let keep: Vec<&str> =
                    on.iter().filter(|i| i.advice == Advice::Keep).map(|i| i.name.as_str()).take(4).collect();
                let mut s = format!("{} startup apps on, {} off.", on.len(), items.len() - on.len());
                if !can_off.is_empty() {
                    s.push_str(&format!(" Safe to turn off: {}.", can_off.join(", ")));
                }
                if !keep.is_empty() {
                    s.push_str(&format!(" Keep on: {}.", keep.join(", ")));
                }
                s
            }
            Tool::Security => match &self.health().security {
                None => "Security status could not be read.".into(),
                Some(sec) => {
                    let yn = |b: Option<bool>| match b {
                        Some(true) => "on",
                        Some(false) => "OFF",
                        None => "unknown",
                    };
                    let fw = if sec.firewall_profiles.is_empty() {
                        "unknown".to_string()
                    } else {
                        sec.firewall_profiles
                            .iter()
                            .map(|(n, on)| format!("{n} {}", if *on { "on" } else { "OFF" }))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    format!(
                        "Microsoft Defender antivirus {}, real-time protection {}, virus definitions {} days old. Firewall: {fw}.",
                        yn(sec.defender_antivirus_enabled),
                        yn(sec.defender_realtime_enabled),
                        sec.defender_signature_age_days.map(|d| d.to_string()).unwrap_or_else(|| "?".into())
                    )
                }
            },
            Tool::Battery => match &self.health().battery {
                None => "No battery (desktop PC) or battery info not available.".into(),
                Some(b) => format!(
                    "Battery {}%, {}. Wear {}.",
                    b.percent.map(|p| p.to_string()).unwrap_or_else(|| "?".into()),
                    match b.charging {
                        Some(true) => "charging",
                        Some(false) => "not charging",
                        None => "charging state unknown",
                    },
                    b.wear_percent().map(|w| format!("{w:.0}%")).unwrap_or_else(|| "unknown".into())
                ),
            },
            Tool::Heat => {
                let t = &self.health().temperatures;
                if t.is_empty() {
                    "Windows did not report temperatures.".into()
                } else {
                    t.iter().take(4).map(|t| format!("{} {:.0}°C", t.label, t.celsius)).collect::<Vec<_>>().join(", ")
                }
            }
            Tool::History => match self.history() {
                None => "No history yet.".into(),
                Some((sum, rows)) => {
                    let recent = rows.iter().take(4).map(|r| r.title.clone()).collect::<Vec<_>>().join("; ");
                    format!(
                        "The app made {} changes so far and freed {} in total. Recent: {}.",
                        sum.changes,
                        gb(sum.bytes_freed),
                        if recent.is_empty() { "none".into() } else { recent }
                    )
                }
            },
            Tool::Problems => {
                let f = self.findings();
                f.iter().take(5).map(|f| format!("- {}", f.title)).collect::<Vec<_>>().join("\n")
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::health::{AppUsage, DiskInfo};

    pub struct FakePc;

    pub fn health() -> HealthSnapshot {
        HealthSnapshot {
            taken_at: chrono::Utc::now(),
            os: "Windows 11 Home".into(),
            host_name: "pc".into(),
            uptime_secs: 9 * 86_400,
            is_admin: false,
            cpu_brand: "Intel Core i3-1005G1".into(),
            cpu_cores: 4,
            cpu_percent: 22.0,
            memory_total: 8 << 30,
            memory_used: (73 << 30) / 10,
            swap_total: 0,
            swap_used: 0,
            process_count: 210,
            disks: vec![DiskInfo {
                mount: "C:\\".into(),
                name: "OS".into(),
                file_system: "NTFS".into(),
                kind: "SSD".into(),
                total_bytes: 237 << 30,
                free_bytes: 41 << 30,
                removable: false,
                is_system: true,
            }],
            gpus: vec![],
            top_memory: vec![
                AppUsage { name: "chrome".into(), processes: 24, memory_bytes: 3 << 30, cpu_percent: 5.0, system: false },
                AppUsage { name: "Teams".into(), processes: 6, memory_bytes: 1 << 30, cpu_percent: 1.0, system: false },
            ],
            top_cpu: vec![],
            temperatures: vec![],
            battery: None,
            security: None,
            physical_disks: vec![],
        }
    }

    impl PcSource for FakePc {
        fn health(&self) -> HealthSnapshot {
            health()
        }
        fn scan(&self) -> Option<ScanReport> {
            None
        }
        fn startup(&self) -> Vec<StartupItem> {
            Vec::new()
        }
        fn history(&self) -> Option<(LedgerSummary, Vec<ChangeRecord>)> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::FakePc;
    use super::*;

    #[test]
    fn tools_describe_facts() {
        let pc = FakePc;
        let f = Facts::new(&pc, crate::Lang::En);
        assert!(f.run(Tool::Specs).contains("Intel Core i3-1005G1"));
        let perf = f.run(Tool::Performance);
        assert!(perf.contains("RAM 91% used"), "{perf}");
        assert!(perf.contains("chrome 3.0 GB"), "{perf}");
        assert!(f.run(Tool::Storage).contains("41.0 GB free"));
        assert!(f.run(Tool::Battery).contains("No battery"));
        assert!(f.run(Tool::Problems).contains("RAM"));
        for t in Tool::ALL {
            assert_eq!(Tool::from_name(t.name()), Some(t));
        }
    }
}
