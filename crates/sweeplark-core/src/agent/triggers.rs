//! Smart alerts: only for problems that last (not a one-second spike), each with a cooldown.

use serde::Serialize;

use super::store::Sample;
use crate::health::HealthSnapshot;
use crate::util::format_bytes;
use crate::{Lang, tr};

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

pub fn evaluate(recent: &[Sample], health: Option<&HealthSnapshot>, new_startup: &[String], lang: Lang) -> Vec<Alert> {
    let mut out = Vec::new();
    let last: Vec<&Sample> = recent.iter().rev().take(SUSTAIN).collect();

    if last.len() == SUSTAIN && last.iter().all(|s| s.ram_percent >= 90.0) {
        let s = last[0];
        let ram = s.ram_percent;
        let app = s.top_ram_app.as_ref().map(|(n, b)| format!("{n} ({})", format_bytes(*b)));
        let mut points = vec![tr!(lang, "RAM {ram:.0}% full for 15 minutes", "RAM {ram:.0}% bhari, 15 minute se")];
        if let Some(a) = &app {
            points.push(tr!(lang, "Most RAM: {a}", "Sabse zyada RAM: {a}"));
        }
        out.push(Alert {
            key: "ram.high".into(),
            cooldown_hours: 4,
            title: tr!(lang, "RAM is {ram:.0}% full", "RAM {ram:.0}% bhari hai"),
            body: match &app {
                Some(a) => tr!(
                    lang,
                    "RAM has been almost full for 15 minutes. {a} is using the most. Close what you are not using.",
                    "15 minute se RAM lagbhag poori bhari hai. Sabse zyada {a} le raha hai. Jo kaam mein nahi hai use band karein."
                ),
                None => tr!(
                    lang,
                    "RAM has been almost full for 15 minutes. Close apps you are not using.",
                    "15 minute se RAM lagbhag poori bhari hai. Jo apps kaam mein nahi hain unhe band karein."
                ),
            },
            points,
        });
    }

    if last.len() == SUSTAIN && last.iter().all(|s| s.cpu_percent >= 85.0) {
        let apps: Vec<&str> = last.iter().filter_map(|s| s.top_cpu_app.as_ref().map(|(n, _)| n.as_str())).collect();
        let same = !apps.is_empty() && apps.iter().all(|a| *a == apps[0]);
        let cpu = last[0].cpu_percent;
        out.push(Alert {
            key: "cpu.high".into(),
            cooldown_hours: 2,
            title: tr!(lang, "CPU has been at full power for a while", "CPU der se poori taakat par chal raha hai"),
            body: if same {
                let a = apps[0];
                tr!(
                    lang,
                    "{a} has used the most CPU for the last 15 minutes. If you are not using it, close it.",
                    "Pichhle 15 minute se {a} CPU sabse zyada le raha hai. Agar yeh aap nahi chala rahe toh ise band karein."
                )
            } else {
                tr!(
                    lang,
                    "CPU has been above 85% for 15 minutes. The PC may get hot and slow.",
                    "Pichhle 15 minute se CPU 85% se upar hai. PC garam aur dheema ho sakta hai."
                )
            },
            points: vec![tr!(lang, "CPU {cpu:.0}% for 15 minutes", "CPU {cpu:.0}%, 15 minute se")],
        });
    }

    if let Some(s) = recent.last()
        && s.system_total > 0
    {
        let free_pct = s.system_free as f64 / s.system_total as f64 * 100.0;
        if free_pct < 10.0 {
            let free = format_bytes(s.system_free);
            out.push(Alert {
                key: "disk.low".into(),
                cooldown_hours: 24,
                title: tr!(lang, "Only {free} left on the system drive", "System drive mein sirf {free} bacha hai"),
                body: tr!(
                    lang,
                    "Low space slows Windows down and can make updates fail. Run a cleanup to free space.",
                    "Jagah kam hone se Windows dheema hota hai aur updates fail ho sakte hain. Clean chala kar jagah banayein."
                ),
                points: vec![tr!(lang, "{free} free ({free_pct:.0}%)", "{free} khaali ({free_pct:.0}%)")],
            });
        }
    }

    for name in new_startup {
        out.push(Alert {
            key: format!("startup.new.{}", name.to_lowercase()),
            cooldown_hours: 24 * 365,
            title: tr!(lang, "New startup app: {name}", "Nayi startup app: {name}"),
            body: tr!(
                lang,
                "{name} will now start with Windows. If you didn't add it or don't need it every day, you can turn it off in Startup.",
                "{name} ab Windows ke saath apne aap chalegi. Agar aapne ise nahi joda ya roz zarurat nahi, toh Startup mein band kar sakte hain."
            ),
            points: vec![tr!(lang, "{name} added to startup", "{name} startup mein judi")],
        });
    }

    if let Some(sec) = health.and_then(|h| h.security.as_ref())
        && (sec.defender_realtime_enabled == Some(false) || sec.firewall_profiles.iter().any(|(_, on)| !on))
    {
        out.push(Alert {
            key: "security.off".into(),
            cooldown_hours: 24,
            title: tr!(lang, "Your PC's protection is weakened", "PC ki suraksha mein kami"),
            body: tr!(
                lang,
                "Defender real-time protection or the firewall is off. If you don't have another antivirus, turn it on.",
                "Defender real-time protection ya Firewall band hai. Agar koi doosra antivirus nahi hai toh ise chalu karein."
            ),
            points: vec![tr!(lang, "Defender or firewall off", "Defender ya firewall band")],
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
        assert!(evaluate(&[s(95.0, 10.0, 50), s(50.0, 10.0, 50), s(95.0, 10.0, 50)], None, &[], Lang::En).is_empty());
        let a = evaluate(&[s(95.0, 90.0, 50), s(92.0, 90.0, 50), s(91.0, 90.0, 5)], None, &["Spotify".into()], Lang::En);
        let keys: Vec<&str> = a.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(keys, vec!["ram.high", "cpu.high", "disk.low", "startup.new.spotify"]);
        assert!(a[0].body.contains("chrome"));
        assert!(a[1].body.contains("game"));
    }
}
