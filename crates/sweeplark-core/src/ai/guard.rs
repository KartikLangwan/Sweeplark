//! Checks on model output before the user sees it. Small models sometimes repeat
//! themselves or make up numbers; if an answer fails, the app shows its own answer instead.

/// Remove `<think>…</think>` blocks some models emit.
pub fn strip_think(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return out.trim().to_string(),
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Cut the text where a line or sentence starts repeating.
pub fn cut_repetition(text: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let key = line.trim().to_lowercase();
        if key.len() >= 12 && !seen.insert(key) {
            break;
        }
        out.push(line);
    }
    out.join("\n").trim().to_string()
}

fn numbers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || (ch == '.' && !cur.is_empty() && !cur.contains('.')) {
            cur.push(ch);
        } else {
            let n = cur.trim_end_matches('.').to_string();
            if !n.is_empty() {
                out.push(n);
            }
            cur.clear();
        }
    }
    out
}

/// "31.0" and "31" are the same number.
fn canonical(n: &str) -> String {
    if n.contains('.') { n.trim_end_matches('0').trim_end_matches('.').to_string() } else { n.to_string() }
}

const UNITS: &[&str] = &["gb", "mb", "kb", "tb", "%", "°", "ghz", "mhz", "din", "day", "hour", "ghant", "min", "sec", "process", "app", "file", "core", "thread"];

/// Small whole numbers (up to 10) that are *not* followed by a unit: counts and list
/// numbers in normal sentences. "9 GB" or "5%" are measurements and must be grounded.
fn harmless_small(answer_lower: &str, n: &str) -> bool {
    if n.contains('.') || n.parse::<f64>().map(|v| v > 10.0).unwrap_or(true) {
        return false;
    }
    let mut from = 0;
    while let Some(i) = answer_lower[from..].find(n) {
        let start = from + i;
        let end = start + n.len();
        let before_digit = answer_lower[..start].chars().next_back().is_some_and(|c| c.is_ascii_digit() || c == '.');
        let after_digit = answer_lower[end..].chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.');
        if !before_digit && !after_digit {
            let rest = answer_lower[end..].trim_start();
            if UNITS.iter().any(|u| rest.starts_with(u)) {
                return false;
            }
        }
        from = end;
    }
    true
}

/// Every measured number in the answer must appear in the facts.
pub fn numbers_grounded(answer: &str, facts: &str) -> bool {
    let known: std::collections::HashSet<String> = numbers(facts).iter().map(|n| canonical(n)).collect();
    let lower = answer.to_lowercase();
    numbers(answer).iter().all(|n| known.contains(&canonical(n)) || harmless_small(&lower, n))
}

/// Phrases where the assistant claims it already changed something. In chat it never acts,
/// so such a sentence is always false.
const ACTION_CLAIMS: &[&str] = &[
    "band kar diya",
    "band kar di ",
    "delete kar diya",
    "delete kar di ",
    "saaf kar diya",
    "hata diya",
    "clean kar diya",
    "maine band",
    "maine delete",
    "maine saaf",
    "i have closed",
    "i closed",
    "i have deleted",
    "i deleted",
    "i have cleaned",
    "i cleaned",
    "i have disabled",
    "email",
    "contact me",
    "customer care",
];

pub fn claims_action(answer: &str) -> bool {
    let a = format!("{} ", answer.to_lowercase());
    ACTION_CLAIMS.iter().any(|c| a.contains(c))
}

const FORBIDDEN: &[&str] = &[
    "registry clean",
    "registry cleaner",
    "ram boost",
    "ram booster",
    "defender band",
    "disable defender",
    "turn off defender",
    "antivirus band",
    "system32",
    "format kar",
    "format your",
];

/// Advice we never give, also when worded differently ("Defender ko band karein",
/// "antivirus hata do", "registry saaf karo"): per sentence, a subject plus an action.
const NEVER: &[(&[&str], &[&str])] = &[
    (&["defender", "antivirus", "anti-virus", "firewall", "smartscreen"], &["band kar", "band karo", "band karein", "off kar", "turn off", "switch off", "disable", "hata", "remove", "uninstall", "bypass"]),
    (&["registry"], &["clean", "saaf", "safai", "optimi", "fix kar"]),
    (&["system32", "winsxs"], &["delete", "hata", "saaf", "clean", "remove"]),
    (&["format"], &["drive", "disk", "pc", "windows", "c:"]),
];

pub fn has_forbidden_advice(answer: &str) -> bool {
    let a = answer.to_lowercase();
    if FORBIDDEN.iter().any(|f| a.contains(f)) {
        return true;
    }
    a.split(['.', '!', '?', '\n', ';']).any(|sentence| {
        NEVER.iter().any(|(subjects, actions)| {
            subjects.iter().any(|s| sentence.contains(s)) && actions.iter().any(|x| sentence.contains(x))
        })
    })
}

/// Clean up and validate. `None` = do not show this answer.
pub fn check(answer: &str, facts: &str) -> Option<String> {
    let text = cut_repetition(&strip_think(answer));
    if text.chars().count() < 8 || has_forbidden_advice(&text) || !numbers_grounded(&text, facts) {
        return None;
    }
    Some(text)
}

/// Stricter check for chat answers: also rejects claims of having done something.
pub fn check_chat(answer: &str, facts: &str) -> Option<String> {
    check(answer, facts).filter(|t| !claims_action(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounding_and_advice_rules() {
        let facts = "RAM 91% used (7.3 GB of 8.0 GB). 214 processes.";
        assert!(numbers_grounded("RAM 91% bhari hai, 7.3 GB use ho rahi hai. 2 tabs band karein.", facts));
        assert!(!numbers_grounded("RAM 91%, 9 GB use ho rahi hai", facts), "made-up measurement");
        assert!(!numbers_grounded("Battery 5% hai", facts));
        assert!(has_forbidden_advice("Defender ko band karein, PC tez hoga."));
        assert!(has_forbidden_advice("Antivirus hata do."));
        assert!(has_forbidden_advice("Registry saaf karo."));
        assert!(!has_forbidden_advice("Defender chalu hai, sab theek hai."));
        assert!(!has_forbidden_advice("Firewall off hai. Use Windows Security mein chalu karein."));
    }

    #[test]
    fn strips_and_cuts() {
        assert_eq!(strip_think("<think>hmm</think>Hello"), "Hello");
        let rep = "RAM 91% bhari hai, Chrome sabse zyada.\nStartup apps kam karein.\nRAM 91% bhari hai, Chrome sabse zyada.\nmore";
        assert_eq!(cut_repetition(rep), "RAM 91% bhari hai, Chrome sabse zyada.\nStartup apps kam karein.");
    }

    #[test]
    fn grounding() {
        let facts = "RAM 91% used (7.3 GB of 8.0 GB). Running 9 days.";
        assert!(numbers_grounded("RAM 91% bhari hai, 7.3 GB use ho rahi hai. 1. Restart karein", facts));
        assert!(!numbers_grounded("RAM 95% bhari hai", facts));
        assert!(!numbers_grounded("PC 15 din se on hai", facts));
        assert!(check("Registry cleaner chalao, PC tez hoga", facts).is_none());
        assert!(check("RAM 91% bhari hai. Chrome band karein.", facts).is_some());
        assert!(numbers_grounded("llama 31 GB", "llama 31.0 GB"));
        assert!(numbers_grounded("RAM 7.30 GB", "RAM 7.3 GB"));
    }

    #[test]
    fn rejects_action_claims_in_chat() {
        let facts = "RAM 91% used";
        assert!(check_chat("Maine heavy apps ko band kar diya hai, RAM 91% thi.", facts).is_none());
        assert!(check_chat("Contact me via email for a report.", facts).is_none());
        assert!(check_chat("RAM 91% bhari hai; Chrome band karein.", facts).is_some());
    }
}
