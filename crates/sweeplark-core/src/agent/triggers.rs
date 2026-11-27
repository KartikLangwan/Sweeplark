//! Smart alerts: only for problems that last (not a one-second spike), each with a cooldown.

use serde::Serialize;

use super::store::Sample;
use crate::health::HealthSnapshot;
use crate::util::format_bytes;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    /// Cooldown key, e.g. "ram.high".
    pub key: String,
    pub cooldown_hours: i64,
    pub title: String,
    pub body: String,
    /// Asking the AI for a friendlier text is worth it for this alert.
    pub points: Vec<String>,
}

/// How many recent samples a problem must last for (3 × 5 min = 15 min).
pub const SUSTAIN: usize = 3;

pub fn evaluate(recent: &[Sample], health: Option<&HealthSnapshot>, new_startup: &[String]) -> Vec<Alert> {
    let mut out = Vec::new();
    let last: Vec<&Sample> = recent.iter().rev().take(SUSTAIN).collect();

    if last.len() == SUSTAIN && last.iter().all(|s| s.ram_percent >= 90.0) {
        let s = last[0];
        let app = s.top_ram_app.as_ref().map(|(n, b)| format!("{n} ({})", format_bytes(*b)));
        out.push(Alert {
            key: "ram.high".into(),
            cooldown_hours: 4,
            title: format!("RAM {:.0}% bhari hai", s.ram_percent),
            body: match &app {
                Some(a) => format!("15 minute se RAM lagbhag poori bhari hai. Sabse zyada {a} le raha hai. Jo kaam mein nahi hai use band karein."),
                None => "15 minute se RAM lagbhag poori bhari hai. Jo apps kaam mein nahi hain unhe band karein.".into(),
            },
            points: vec![format!("RAM {:.0}% bhari, 15 minute se", s.ram_percent)].into_iter().chain(app.map(|a| format!("Sabse zyada RAM: {a}"))).collect(),
        });
    }

    if last.len() == SUSTAIN && last.iter().all(|s| s.cpu_percent >= 85.0) {
        let apps: Vec<&str> = last.iter().filter_map(|s| s.top_cpu_app.as_ref().map(|(n, _)| n.as_str())).collect();
        let same = !apps.is_empty() && apps.iter().all(|a| *a == apps[0]);
        out.push(Alert {
            key: "cpu.high".into(),
            cooldown_hours: 2,
            title: "CPU der se poori taakat par chal raha hai".into(),
            body: if same {
                format!("Pichhle 15 minute se {} CPU sabse zyada le raha hai. Agar yeh aap nahi chala rahe toh ise band karein.", apps[0])
            } else {
                "Pichhle 15 minute se CPU 85% se upar hai. PC garam aur dheema ho sakta hai.".into()
            },
            points: vec![format!("CPU {:.0}%, 15 minute se", last[0].cpu_percent)],
        });
    }

    if let Some(s) = recent.last()
        && s.system_total > 0
    {
        let free_pct = s.system_free as f64 / s.system_total as f64 * 100.0;
        if free_pct < 10.0 {
            out.push(Alert {
                key: "disk.low".into(),
                cooldown_hours: 24,
                title: format!("System drive mein sirf {} bacha hai", format_bytes(s.system_free)),
                body: "Jagah kam hone se Windows dheema hota hai aur updates fail ho sakte hain. Clean chala kar jagah banayein.".into(),
                points: vec![format!("{} khaali ({free_pct:.0}%)", format_bytes(s.system_free))],
            });
        }
    }

    for name in new_startup {
        out.push(Alert {
            key: format!("startup.new.{}", name.to_lowercase()),
            cooldown_hours: 24 * 365,
            title: format!("Nayi startup app: {name}"),
            body: format!("{name} ab Windows ke saath apne aap chalegi. Agar aapne ise nahi joda ya roz zarurat nahi, toh Startup mein band kar sakte hain."),
            points: vec![format!("{name} startup mein judi")],
        });
    }

    if let Some(sec) = health.and_then(|h| h.security.as_ref())
        && (sec.defender_realtime_enabled == Some(false) || sec.firewall_profiles.iter().any(|(_, on)| !on))
    {
        out.push(Alert {
            key: "security.off".into(),
            cooldown_hours: 24,
            title: "PC ki suraksha mein kami".into(),
            body: "Defender real-time protection ya Firewall band hai. Agar koi doosra antivirus nahi hai toh ise chalu karein.".into(),
            points: vec!["Defender ya firewall band".into()],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn s(ram: f32, cpu: f32, free: u64) -> Sample {
        Sample {
            at: Utc::now(),
            cpu_percent: cpu,
            ram_percent: ram,
            ram_used: 0,
            system_free: free,
            system_total: 100,
            top_ram_app: Some(("chrome".into(), 3 << 30)),
            top_cpu_app: Some(("game".into(), 70.0)),
        }
    }

    #[test]
    fn needs_sustained_problem() {
        assert!(evaluate(&[s(95.0, 10.0, 50), s(50.0, 10.0, 50), s(95.0, 10.0, 50)], None, &[]).is_empty());
        let a = evaluate(&[s(95.0, 90.0, 50), s(92.0, 90.0, 50), s(91.0, 90.0, 5)], None, &["Spotify".into()]);
        let keys: Vec<&str> = a.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(keys, vec!["ram.high", "cpu.high", "disk.low", "startup.new.spotify"]);
        assert!(a[0].body.contains("chrome"));
        assert!(a[1].body.contains("game"));
    }
}
