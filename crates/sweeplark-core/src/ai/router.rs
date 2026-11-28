//! Picks the tools for a question with plain keyword rules (English, Hinglish and Hindi).
//! Works the same for every model size; bigger models may add tools on top.

use std::collections::BTreeSet;

use super::facts::Tool;

const RULES: &[(Tool, &[&str])] = &[
    (
        Tool::Performance,
        &[
            "slow", "dheem", "dhim", "hang", "lag", "atak", "tez", "speed", "fast", "ram", "memory", "cpu", "process",
            "heavy", "bhari", "freeze", "garbad", "chal raha", "धीम", "स्लो", "हैंग", "रैम", "तेज", "अटक",
        ],
    ),
    (
        Tool::Storage,
        &[
            "space", "jagah", "storage", "disk", "drive", "bhar", "full", "gb", "junk", "kachra", "clean", "saaf", "safai",
            "delete", "temp", "cache", "जगह", "स्टोरेज", "डिस्क", "सफाई", "साफ",
        ],
    ),
    (Tool::Startup, &["startup", "start up", "boot", "start hone", "on hone", "chalu hote", "स्टार्टअप", "बूट"]),
    (
        Tool::Security,
        &["virus", "secur", "defender", "firewall", "hack", "antivirus", "suraksha", "malware", "वायरस", "सुरक्षा"],
    ),
    (Tool::Battery, &["battery", "charge", "charging", "betri", "batri", "बैटरी", "चार्ज"]),
    (Tool::Heat, &["garam", "heat", "temp ", "temperature", "fan", "hot", "garmi", "गरम", "गर्म", "तापमान"]),
    (
        Tool::Specs,
        &[
            "spec", "config", "processor", "kitni ram", "graphics", "gpu", "model", "version", "konsa", "kaunsa",
            "hardware", "स्पेक", "प्रोसेसर",
        ],
    ),
    (
        Tool::History,
        &[
            "kya kiya", "history", "badlav", "pichl", "summary", "report", "aaj", "hafte", "week", "today", "changed",
            "रिपोर्ट", "आज", "हफ्ते",
        ],
    ),
    (
        Tool::Optimizer,
        &[
            "optimi", "tweak", "setting", "fast kar", "tez kar", "speed up", "boost", "telemetry", "privacy", "game mode",
            "fps", "gaming", "ऑप्टिम", "तेज कर",
        ],
    ),
    (Tool::Graphics, &["gpu", "graphics", "vram", "video memory", "nvidia", "radeon", "game", "fps", "ग्राफिक"]),
    (
        Tool::Background,
        &[
            "ram", "memory", "background", "boost", "khaali", "free ram", "hang", "lag", "slow", "dheem", "game", "fps",
            "बैकग्राउंड", "रैम",
        ],
    ),
    (
        Tool::Apps,
        &["uninstall", "install", "program", "software", "app hata", "apps", "update", "अनइंस्टॉल", "ऐप"],
    ),
];

/// Tools to run for this question. Specs and Problems are always included: they are short
/// and keep every answer grounded in this PC.
pub fn route(question: &str) -> BTreeSet<Tool> {
    let q = format!(" {} ", question.to_lowercase());
    let mut tools: BTreeSet<Tool> = [Tool::Specs, Tool::Problems].into();
    for (tool, words) in RULES {
        if words.iter().any(|w| crate::util::has_word(&q, w.trim())) {
            tools.insert(*tool);
        }
    }
    tools
}

/// True when only the default tools matched (the question is vague or off-topic).
pub fn is_vague(tools: &BTreeSet<Tool>) -> bool {
    tools.iter().all(|t| matches!(t, Tool::Specs | Tool::Problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_hinglish_english_hindi() {
        assert!(route("mera PC slow kyun hai").contains(&Tool::Performance));
        assert!(route("C drive bhar gayi").contains(&Tool::Storage));
        assert!(route("मेरा पीसी धीमा है").contains(&Tool::Performance));
        assert!(route("is my antivirus on?").contains(&Tool::Security));
        assert!(route("battery kitni ghis gayi").contains(&Tool::Battery));
        assert!(route("aaj kya kiya tumne").contains(&Tool::History));
        let t = route("tum kaun ho");
        assert!(is_vague(&t));
    }
}
