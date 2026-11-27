//! Daily and weekly summaries, built from samples and the change ledger.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::store::Sample;
use crate::ledger::ChangeRecord;
use crate::util::format_bytes;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PeriodSummary {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub samples: usize,
    pub avg_cpu: f32,
    pub peak_cpu: f32,
    pub avg_ram: f32,
    pub peak_ram: f32,
    pub peak_ram_app: Option<String>,
    /// Apps most often at the top of RAM use.
    pub heavy_apps: Vec<(String, u32)>,
    pub freed_bytes: u64,
    pub cleanups: u32,
    pub changes: u32,
    pub alerts: u32,
    pub disk_free_start: Option<u64>,
    pub disk_free_end: Option<u64>,
}

pub fn build(from: DateTime<Utc>, to: DateTime<Utc>, samples: &[Sample], changes: &[ChangeRecord], alerts: u32) -> PeriodSummary {
    let mut s = PeriodSummary { from, to, samples: samples.len(), alerts, ..Default::default() };
    if !samples.is_empty() {
        let n = samples.len() as f32;
        s.avg_cpu = samples.iter().map(|x| x.cpu_percent).sum::<f32>() / n;
        s.avg_ram = samples.iter().map(|x| x.ram_percent).sum::<f32>() / n;
        s.peak_cpu = samples.iter().map(|x| x.cpu_percent).fold(0.0, f32::max);
        let peak = samples.iter().max_by(|a, b| a.ram_percent.total_cmp(&b.ram_percent)).expect("non-empty");
        s.peak_ram = peak.ram_percent;
        s.peak_ram_app = peak.top_ram_app.as_ref().map(|(n, _)| n.clone());
        let mut counts: HashMap<&str, u32> = HashMap::new();
        for x in samples {
            if let Some((n, _)) = &x.top_ram_app {
                *counts.entry(n.as_str()).or_default() += 1;
            }
        }
        let mut heavy: Vec<(String, u32)> = counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        heavy.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        heavy.truncate(3);
        s.heavy_apps = heavy;
        s.disk_free_start = samples.first().map(|x| x.system_free);
        s.disk_free_end = samples.last().map(|x| x.system_free);
    }
    for c in changes {
        s.changes += 1;
        if c.category == "clean" {
            s.cleanups += 1;
            s.freed_bytes += c.bytes_freed;
        }
    }
    s
}

impl PeriodSummary {
    /// (title, body, points for the AI). Text is Hinglish and uses only measured numbers.
    pub fn render(&self, weekly: bool) -> (String, String, Vec<String>) {
        let when = if weekly { "Is hafte" } else { "Aaj" };
        let title = if weekly { "Hafte ki report".to_string() } else { "Aaj ki report".to_string() };
        let mut points = Vec::new();
        if self.freed_bytes > 0 {
            points.push(format!("{when} {} jagah khaali hui ({} safai)", format_bytes(self.freed_bytes), self.cleanups));
        } else {
            points.push(format!("{when} koi safai nahi hui"));
        }
        if self.samples > 0 {
            let app = self.peak_ram_app.as_deref().map(|a| format!(" ({a} sabse bhari)")).unwrap_or_default();
            points.push(format!("RAM ausat {:.0}%, sabse zyada {:.0}%{app}", self.avg_ram, self.peak_ram));
            points.push(format!("CPU ausat {:.0}%, sabse zyada {:.0}%", self.avg_cpu, self.peak_cpu));
        }
        if let (Some(a), Some(b)) = (self.disk_free_start, self.disk_free_end) {
            if b + 1024 * 1024 * 1024 < a {
                points.push(format!("System drive ki khaali jagah {} se {} hui", format_bytes(a), format_bytes(b)));
            } else {
                points.push(format!("System drive mein {} khaali hai", format_bytes(b)));
            }
        }
        if self.alerts > 0 {
            points.push(format!("{} alert bheje gaye", self.alerts));
        }
        if self.changes > self.cleanups {
            points.push(format!("{} doosre badlav (startup vagairah)", self.changes - self.cleanups));
        }
        let body = points.iter().map(|p| format!("• {p}")).collect::<Vec<_>>().join("\n");
        (title, body, points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Risk;
    use crate::ledger::ChangeStatus;

    #[test]
    fn builds_and_renders() {
        let now = Utc::now();
        let mk = |ram: f32, app: &str, free: u64| Sample {
            at: now,
            cpu_percent: 20.0,
            ram_percent: ram,
            ram_used: 0,
            system_free: free,
            system_total: 100 << 30,
            top_ram_app: Some((app.into(), 1 << 30)),
            top_cpu_app: None,
        };
        let samples = vec![mk(60.0, "chrome", 40 << 30), mk(91.0, "chrome", 39 << 30), mk(70.0, "Teams", 35 << 30)];
        let change = ChangeRecord {
            id: 1,
            at: now,
            category: "clean".into(),
            title: "x".into(),
            risk: Risk::Safe,
            status: ChangeStatus::Applied,
            bytes_freed: 1 << 30,
            items: 10,
            details: serde_json::Value::Null,
            revert: vec![],
            can_undo: false,
        };
        let s = build(now, now, &samples, &[change], 1);
        assert_eq!(s.peak_ram, 91.0);
        assert_eq!(s.peak_ram_app.as_deref(), Some("chrome"));
        assert_eq!(s.heavy_apps[0], ("chrome".to_string(), 2));
        let (title, body, points) = s.render(false);
        assert_eq!(title, "Aaj ki report");
        assert!(body.contains("1.0 GB jagah khaali hui"));
        assert!(body.contains("40.0 GB se 35.0 GB"));
        assert!(points.len() >= 4);
    }
}
