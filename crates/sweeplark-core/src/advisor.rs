//! Offline advisor: turns measurements into plain Hinglish advice.
//!
//! It is rule based on purpose: every sentence comes from a real number on this PC,
//! so it can explain itself and never invents problems. It only *suggests* actions.

use serde::Serialize;

use crate::cleaner::ScanReport;
use crate::health::HealthSnapshot;
use crate::startup::{Advice, StartupItem};
use crate::util::format_bytes;
use crate::{Lang, Risk, tr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Good,
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Performance,
    Storage,
    Security,
    Battery,
    Heat,
    Hardware,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SuggestedAction {
    Clean,
    ReviewStartup,
    Restart,
    OpenWindowsSecurity,
    CloseApps { apps: Vec<String> },
    Backup,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub topics: Vec<Topic>,
    pub title: String,
    pub detail: String,
    pub action: Option<SuggestedAction>,
}

fn finding(
    id: &str,
    severity: Severity,
    topics: &[Topic],
    title: String,
    detail: String,
    action: Option<SuggestedAction>,
) -> Finding {
    Finding { id: id.into(), severity, topics: topics.to_vec(), title, detail, action }
}

fn list_apps(apps: &[(String, String)]) -> String {
    apps.iter().map(|(n, v)| format!("{n} ({v})")).collect::<Vec<_>>().join(", ")
}

/// Look at everything and list what deserves attention, most serious first.
pub fn analyze(
    health: &HealthSnapshot,
    scan: Option<&ScanReport>,
    startup: Option<&[StartupItem]>,
    lang: Lang,
) -> Vec<Finding> {
    use Severity::*;
    use Topic::*;
    let mut out = Vec::new();

    // RAM
    let mem = health.memory_percent();
    if mem >= 80.0 {
        let heavy: Vec<(String, String)> = health
            .top_memory
            .iter()
            .filter(|a| !a.system)
            .take(3)
            .map(|a| (a.name.clone(), format_bytes(a.memory_bytes)))
            .collect();
        let apps = list_apps(&heavy);
        out.push(finding(
            "ram.high",
            if mem >= 90.0 { Critical } else { Warning },
            &[Performance],
            tr!(lang, "RAM is {mem:.0}% full", "RAM {mem:.0}% bhari hai"),
            tr!(
                lang,
                "When RAM is full, Windows uses the disk as memory and the PC slows down. Using the most RAM: {apps}. Close what you are not using.",
                "Jab RAM bhar jaati hai, Windows disk ko RAM ki tarah use karta hai aur PC dheema ho jata hai. Sabse zyada RAM: {apps}. Jo kaam mein nahi hai use band kar dein."
            ),
            Some(SuggestedAction::CloseApps { apps: heavy.into_iter().map(|(n, _)| n).collect() }),
        ));
    }

    // CPU (a single moment, so only flag it when it is really busy)
    if health.cpu_percent >= 85.0 {
        let busy: Vec<(String, String)> = health
            .top_cpu
            .iter()
            .filter(|a| a.cpu_percent >= 5.0 && !a.system)
            .take(3)
            .map(|a| (a.name.clone(), format!("{:.0}%", a.cpu_percent)))
            .collect();
        let cpu = health.cpu_percent;
        let apps = list_apps(&busy);
        out.push(finding(
            "cpu.high",
            Warning,
            &[Performance, Heat],
            tr!(lang, "CPU is {cpu:.0}% busy", "CPU {cpu:.0}% busy hai"),
            if busy.is_empty() {
                tr!(lang, "The processor is working at full power right now.", "Processor abhi poori taakat se kaam kar raha hai.")
            } else {
                tr!(lang, "The processor is working at full power. Busiest: {apps}.", "Processor abhi poori taakat se kaam kar raha hai. Sabse zyada: {apps}.")
            },
            (!busy.is_empty()).then(|| SuggestedAction::CloseApps { apps: busy.into_iter().map(|(n, _)| n).collect() }),
        ));
    }

    // System drive space
    if let Some(disk) = health.system_disk() {
        let free_pct = 100.0 - disk.used_percent();
        let drive = disk.mount.trim_end_matches('\\').to_string();
        let free = format_bytes(disk.free_bytes);
        if free_pct < 20.0 {
            out.push(finding(
                "disk.low",
                if free_pct < 10.0 { Critical } else { Warning },
                &[Storage, Performance],
                tr!(lang, "Only {free} left on {drive}", "{drive} drive mein sirf {free} bacha hai"),
                tr!(
                    lang,
                    "The drive is only {free_pct:.0}% free. Windows needs space for updates and temporary files; below 10% the PC slows down and updates can fail.",
                    "Drive {free_pct:.0}% hi khaali hai. Windows ko updates aur temporary files ke liye jagah chahiye; 10% se kam jagah par PC dheema ho jata hai aur updates fail ho sakte hain."
                ),
                Some(SuggestedAction::Clean),
            ));
        }
        if disk.kind == "HDD" {
            out.push(finding(
                "disk.hdd",
                Info,
                &[Performance, Hardware],
                tr!(lang, "Windows runs on an older hard disk (HDD)", "Windows purani hard disk (HDD) par chal raha hai"),
                tr!(
                    lang,
                    "We will make it as fast as software can, but the biggest upgrade is an SSD: the PC would start 3 to 5 times faster and apps would open instantly.",
                    "Software se jitna tez ho sakta hai, utna hum karenge. Par sabse bada fark SSD lagwane se padega: PC 3 se 5 guna jaldi start hoga aur apps turant khulengi."
                ),
                None,
            ));
        }
    }

    // Junk found by the cleaner
    if let Some(scan) = scan {
        let junk = scan.ready_bytes(Risk::Ask);
        if junk >= 200 * 1024 * 1024 {
            let size = format_bytes(junk);
            out.push(finding(
                "junk.found",
                if junk >= 2 * 1024 * 1024 * 1024 { Warning } else { Info },
                &[Storage],
                tr!(lang, "{size} of junk files found", "{size} bekaar files mili"),
                tr!(
                    lang,
                    "Temporary files, caches and old logs. Removing them does not touch your personal files.",
                    "Temp files, cache aur purane logs. Inhe hatane se personal files ko kuch nahi hoga."
                ),
                Some(SuggestedAction::Clean),
            ));
        }
    }

    // Startup
    if let Some(items) = startup {
        let enabled: Vec<&StartupItem> = items.iter().filter(|i| i.enabled).collect();
        let suggest: Vec<&str> =
            enabled.iter().filter(|i| i.advice == Advice::Disable).map(|i| i.name.as_str()).collect();
        if enabled.len() >= 6 || !suggest.is_empty() {
            let n = enabled.len();
            let names = suggest.iter().take(5).copied().collect::<Vec<_>>().join(", ");
            let detail = if suggest.is_empty() {
                tr!(
                    lang,
                    "{n} apps start when the PC turns on. Turning off the ones you don't need every day makes startup faster.",
                    "{n} apps PC on hote hi chalti hain. Jo roz nahi chahiye unhe band karne se PC jaldi start hoga."
                )
            } else {
                tr!(
                    lang,
                    "{n} apps start when the PC turns on. Safe to turn off: {names}. You can still open them yourself any time.",
                    "{n} apps PC on hote hi chalti hain. Inhe band karna safe hai: {names}. Band karne par bhi aap inhe kabhi bhi khud khol sakte hain."
                )
            };
            out.push(finding(
                "startup.many",
                if n >= 10 { Warning } else { Info },
                &[Performance],
                tr!(lang, "{n} startup apps are on", "{n} startup apps chalu hain"),
                detail,
                Some(SuggestedAction::ReviewStartup),
            ));
        }
    }

    // Uptime
    let days = health.uptime_secs / 86_400;
    if days >= 7 {
        out.push(finding(
            "uptime.long",
            Info,
            &[Performance],
            tr!(lang, "The PC has not restarted in {days} days", "PC {days} din se restart nahi hua"),
            tr!(
                lang,
                "A restart clears stuck memory and installs pending updates.",
                "Restart karne se atki hui memory saaf hoti hai aur pending updates install hote hain."
            ),
            Some(SuggestedAction::Restart),
        ));
    }

    // Small RAM
    let gb = health.memory_total as f64 / 1024f64.powi(3);
    if gb > 0.0 && gb < 6.0 {
        out.push(finding(
            "ram.small",
            Info,
            &[Performance, Hardware],
            tr!(lang, "This PC has {gb:.0} GB of RAM", "PC mein {gb:.0} GB RAM hai"),
            tr!(
                lang,
                "That is little for today's browsers and apps. Keep fewer tabs open and fewer startup apps. Upgrading to 8 GB is the cheapest big improvement.",
                "Aaj ke browsers aur apps ke liye yeh kam hai. Kam tabs khulein, startup apps kam rakhein. Ho sake toh 8 GB RAM lagwana sabse sasta upgrade hai."
            ),
            None,
        ));
    }

    // Security
    if let Some(sec) = &health.security {
        if sec.defender_realtime_enabled == Some(false) || sec.defender_antivirus_enabled == Some(false) {
            out.push(finding(
                "security.defender_off",
                Critical,
                &[Security],
                tr!(lang, "Microsoft Defender real-time protection is off", "Microsoft Defender ki real-time suraksha band hai"),
                tr!(
                    lang,
                    "If you don't have another antivirus, turn it on right away.",
                    "Agar aapke paas koi doosra antivirus nahi hai toh ise turant chalu karein."
                ),
                Some(SuggestedAction::OpenWindowsSecurity),
            ));
        } else if let Some(age) = sec.defender_signature_age_days
            && age > 7
        {
            out.push(finding(
                "security.signatures_old",
                Warning,
                &[Security],
                tr!(lang, "Antivirus definitions are {age} days old", "Antivirus ki jaankari {age} din purani hai"),
                tr!(
                    lang,
                    "Defender needs updates to recognise new threats. Run Windows Update.",
                    "Defender ko naye viruses pehchanne ke liye update chahiye. Windows Update chalayein."
                ),
                Some(SuggestedAction::OpenWindowsSecurity),
            ));
        }
        let off: Vec<&str> = sec.firewall_profiles.iter().filter(|(_, on)| !on).map(|(n, _)| n.as_str()).collect();
        if !off.is_empty() {
            let profiles = off.join(", ");
            out.push(finding(
                "security.firewall_off",
                Warning,
                &[Security],
                tr!(lang, "Firewall is off ({profiles})", "Firewall band hai ({profiles})"),
                tr!(
                    lang,
                    "The firewall blocks unknown incoming connections. Keep it on.",
                    "Firewall bahar se aane wale anjaan connections rokta hai. Ise chalu rakhein."
                ),
                Some(SuggestedAction::OpenWindowsSecurity),
            ));
        }
    }

    // Disk hardware health
    for d in &health.physical_disks {
        if d.health == "Warning" || d.health == "Unhealthy" {
            let name = &d.name;
            out.push(finding(
                "disk.health",
                Critical,
                &[Hardware, Storage],
                tr!(lang, "Disk '{name}' shows signs of failing", "Disk '{name}' kharab hone ke sanket de rahi hai"),
                tr!(
                    lang,
                    "Windows reports this disk as unhealthy. Back up important files now.",
                    "Windows ke hisaab se is disk ki sehat theek nahi hai. Zaruri files ka backup abhi le lein."
                ),
                Some(SuggestedAction::Backup),
            ));
        }
    }

    // Battery
    if let Some(wear) = health.battery.as_ref().and_then(|b| b.wear_percent())
        && wear >= 20.0
    {
        let left = 100.0 - wear;
        out.push(finding(
            "battery.wear",
            if wear >= 40.0 { Warning } else { Info },
            &[Battery, Hardware],
            tr!(lang, "Battery is {wear:.0}% worn", "Battery {wear:.0}% ghis chuki hai"),
            tr!(
                lang,
                "It now holds {left:.0}% of its original charge. Above 40% wear, consider replacing it.",
                "Nayi battery ke muqable ab sirf {left:.0}% charge pakadti hai. 40% se zyada ghisawat par battery badalne ka sochein."
            ),
            None,
        ));
    }

    // Heat
    if let Some(hot) = health.temperatures.iter().max_by(|a, b| a.celsius.total_cmp(&b.celsius))
        && hot.celsius >= 90.0
    {
        let (label, c) = (&hot.label, hot.celsius);
        out.push(finding(
            "heat.high",
            Warning,
            &[Heat, Hardware],
            tr!(lang, "{label} is running hot ({c:.0}°C)", "{label} {c:.0}°C tak garam hai"),
            tr!(
                lang,
                "Heat makes the PC slow itself down. Clean the vents and don't use a laptop on a bed or cushion.",
                "Zyada garmi se PC apni speed khud kam kar deta hai. Hawa ke raaste (vents) saaf karein, laptop ko bistar par na chalayein."
            ),
            None,
        ));
    }

    if !out.iter().any(|f| f.severity >= Warning) {
        out.push(finding(
            "all.good",
            Good,
            &[Performance, Storage, Security],
            tr!(lang, "Your PC is in good shape", "PC ki sehat achhi hai"),
            tr!(
                lang,
                "No big problems found. A quick scan once a week is enough.",
                "Koi badi dikkat nahi mili. Hafte mein ek baar scan kar lena kaafi hai."
            ),
            None,
        ));
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub text: String,
    pub findings: Vec<Finding>,
}

fn topic_for(question: &str) -> Option<Topic> {
    let q = question.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| q.contains(w));
    if has(&["battery", "charge", "charging", "betri"]) {
        Some(Topic::Battery)
    } else if has(&["garam", "heat", "temp", "fan", "hot", "garmi"]) {
        Some(Topic::Heat)
    } else if has(&["virus", "secur", "safe", "defender", "firewall", "hack", "antivirus", "suraksha"]) {
        Some(Topic::Security)
    } else if has(&["space", "jagah", "storage", "disk", "bhar", "full", "gb", "drive"]) {
        Some(Topic::Storage)
    } else if has(&["slow", "dheem", "dhim", "hang", "lag", "atak", "tez", "speed", "fast", "boot", "start"]) {
        Some(Topic::Performance)
    } else {
        None
    }
}

/// Answer a free-text question ("Why is my PC slow?") from the findings. Offline, no AI model.
pub fn ask(question: &str, findings: &[Finding], lang: Lang) -> Answer {
    let topic = topic_for(question);
    let relevant: Vec<Finding> = findings
        .iter()
        .filter(|f| f.severity != Severity::Good)
        .filter(|f| topic.is_none_or(|t| f.topics.contains(&t)))
        .take(4)
        .cloned()
        .collect();
    let intro = match topic {
        Some(Topic::Performance) => lang.pick("Here is what is slowing your PC down", "PC ke dheeme hone ki wajahein jo mujhe mili"),
        Some(Topic::Storage) => lang.pick("About storage space", "Jagah ke baare mein"),
        Some(Topic::Security) => lang.pick("About security", "Suraksha ke baare mein"),
        Some(Topic::Battery) => lang.pick("About the battery", "Battery ke baare mein"),
        Some(Topic::Heat) => lang.pick("About heat", "Garmi ke baare mein"),
        Some(Topic::Hardware) | None => lang.pick("The most important things about your PC", "Aapke PC ki sabse zaruri baatein"),
    };
    let text = if relevant.is_empty() {
        match topic {
            Some(Topic::Performance) => lang
                .pick(
                    "I don't see a big cause right now: RAM, CPU, disk and startup all look fine. If one app is slow, close and reopen it, or restart the PC.",
                    "Abhi koi badi wajah nahi dikhi: RAM, CPU, disk aur startup sab theek hain. Agar kisi khaas app mein dikkat hai toh use band karke dobara kholein, ya PC restart karein.",
                )
                .to_string(),
            Some(Topic::Battery) => lang.pick("Nothing worrying about the battery.", "Battery ke baare mein koi chinta wali baat nahi mili.").into(),
            Some(Topic::Heat) => lang
                .pick("Temperatures look fine, or Windows did not report them.", "Temperature theek hai, ya Windows ne sensor ki jaankari nahi di.")
                .into(),
            _ => lang.pick("No big problems found. Everything looks fine.", "Koi badi dikkat nahi mili. Sab theek lag raha hai.").into(),
        }
    } else {
        let mut t = format!("{intro}:\n");
        for (i, f) in relevant.iter().enumerate() {
            t.push_str(&format!("{}. {} — {}\n", i + 1, f.title, f.detail));
        }
        t.push('\n');
        t.push_str(lang.pick(
            "I won't delete anything myself. You decide after looking.",
            "Main khud kuch delete nahi karunga. Dekh kar faisla aap karein.",
        ));
        t
    };
    Answer { text, findings: relevant }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{AppUsage, DiskInfo};
    use crate::platform::system::SecurityStatus;

    fn health() -> HealthSnapshot {
        HealthSnapshot {
            taken_at: chrono::Utc::now(),
            os: "Windows 11".into(),
            host_name: "pc".into(),
            uptime_secs: 10 * 86_400,
            is_admin: false,
            cpu_brand: "Intel".into(),
            cpu_cores: 4,
            cpu_percent: 20.0,
            memory_total: 8 * 1024 * 1024 * 1024,
            memory_used: 7 * 1024 * 1024 * 1024 + 600 * 1024 * 1024,
            swap_total: 0,
            swap_used: 0,
            process_count: 200,
            disks: vec![DiskInfo {
                mount: "C:\\".into(),
                name: "OS".into(),
                file_system: "NTFS".into(),
                kind: "HDD".into(),
                total_bytes: 100,
                free_bytes: 5,
                removable: false,
                is_system: true,
            }],
            gpus: vec![],
            top_memory: vec![AppUsage { name: "chrome".into(), processes: 20, memory_bytes: 3 << 30, cpu_percent: 5.0, system: false }],
            top_cpu: vec![],
            temperatures: vec![],
            battery: None,
            security: Some(SecurityStatus {
                defender_antivirus_enabled: Some(true),
                defender_realtime_enabled: Some(false),
                defender_signature_age_days: Some(1),
                firewall_profiles: vec![("Public".into(), true)],
            }),
            physical_disks: vec![],
        }
    }

    #[test]
    fn finds_real_problems_in_order() {
        let f = analyze(&health(), None, None, Lang::En);
        let ids: Vec<&str> = f.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"ram.high"));
        assert!(ids.contains(&"disk.low"));
        assert!(ids.contains(&"disk.hdd"));
        assert!(ids.contains(&"uptime.long"));
        assert!(ids.contains(&"security.defender_off"));
        assert!(!ids.contains(&"all.good"));
        assert_eq!(f[0].severity, Severity::Critical);
        assert!(f.windows(2).all(|w| w[0].severity >= w[1].severity));
    }

    #[test]
    fn healthy_pc_says_so() {
        let mut h = health();
        h.memory_used = 2 << 30;
        h.disks[0].free_bytes = 60;
        h.disks[0].kind = "SSD".into();
        h.uptime_secs = 3600;
        h.security = None;
        let f = analyze(&h, None, None, Lang::En);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "all.good");
    }

    #[test]
    fn answers_by_topic() {
        let f = analyze(&health(), None, None, Lang::Hinglish);
        let a = ask("mera PC slow kyun hai?", &f, Lang::Hinglish);
        assert!(a.findings.iter().all(|x| x.topics.contains(&Topic::Performance)));
        assert!(a.text.contains("RAM"));
        let s = ask("virus se safe hoon?", &f, Lang::Hinglish);
        assert_eq!(s.findings[0].id, "security.defender_off");
        let b = ask("battery kaisi hai", &f, Lang::Hinglish);
        assert!(b.findings.is_empty());
        assert!(b.text.contains("Battery"));
        let en = ask("why is my pc slow", &analyze(&health(), None, None, Lang::En), Lang::En);
        assert!(en.text.starts_with("Here is what is slowing"));
        assert!(en.text.contains("RAM is"));
    }
}
