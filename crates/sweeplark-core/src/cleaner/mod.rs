//! Cleaner: rule definitions (TOML data files) and the scan → preview → clean engine.

pub mod classify;
mod engine;
pub mod recycle;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub use engine::*;

use crate::{Error, Result, Risk};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    System,
    Browser,
    Apps,
    Games,
    Developer,
    Logs,
    RecycleBin,
    Privacy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    #[default]
    Files,
    RecycleBin,
}

fn yes() -> bool {
    true
}

fn windows_only() -> Vec<String> {
    vec!["windows".into()]
}

/// One place to clean. `path` may contain `%ENV%` variables and `*` wildcards in folder names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    #[serde(default)]
    pub kind: TargetKind,
    #[serde(default)]
    pub path: String,
    #[serde(default = "yes")]
    pub recursive: bool,
    /// Globs relative to the target folder. Empty = everything.
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Skip files changed more recently than this (files still in use).
    #[serde(default)]
    pub min_age_hours: u64,
    /// Remove sub-folders left empty after cleaning (never the target folder itself).
    #[serde(default)]
    pub remove_empty_dirs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanerDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_hi: Option<String>,
    pub description: String,
    #[serde(default)]
    pub description_hi: Option<String>,
    pub category: Category,
    pub risk: Risk,
    #[serde(default)]
    pub needs_admin: bool,
    /// Deletes browsing data (history, cookies). Needs the user's consent first.
    #[serde(default)]
    pub privacy: bool,
    /// Skip this cleaner while any of these programs runs.
    #[serde(default)]
    pub running: Vec<String>,
    #[serde(default = "yes")]
    pub default_enabled: bool,
    /// Shown to the user, e.g. "You will be logged out of websites".
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub note_hi: Option<String>,
    #[serde(default = "windows_only")]
    pub os: Vec<String>,
    /// Check every file's type: anything that looks like the user's own file (document,
    /// photo, installer ...) is kept back for approval. Used for Temp folders.
    #[serde(default)]
    pub inspect: bool,
    #[serde(rename = "target")]
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    #[serde(rename = "cleaner")]
    cleaners: Vec<CleanerDef>,
}

/// Built-in rule files, embedded in the binary.
const BUILTIN: &[(&str, &str)] = &[
    ("system.toml", include_str!("../../../../rules/cleaners/system.toml")),
    ("browsers.toml", include_str!("../../../../rules/cleaners/browsers.toml")),
    ("apps.toml", include_str!("../../../../rules/cleaners/apps.toml")),
    ("games.toml", include_str!("../../../../rules/cleaners/games.toml")),
    ("developer.toml", include_str!("../../../../rules/cleaners/developer.toml")),
    ("privacy.toml", include_str!("../../../../rules/cleaners/privacy.toml")),
];

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub cleaners: Vec<CleanerDef>,
}

impl RuleSet {
    pub fn builtin() -> Self {
        let mut set = RuleSet::default();
        for (name, text) in BUILTIN {
            // Built-in rules are tested in CI, so a parse error here is a programming bug.
            set.add_file(name, text).unwrap_or_else(|e| panic!("built-in rules broken: {e}"));
        }
        set
    }

    /// Add rules from one TOML file, validating them.
    pub fn add_file(&mut self, file: &str, text: &str) -> Result<()> {
        let err = |message: String| Error::Rules { file: file.to_string(), message };
        let parsed: RuleFile = toml::from_str(text).map_err(|e| err(e.to_string()))?;
        let mut seen: HashSet<String> = self.cleaners.iter().map(|c| c.id.clone()).collect();
        for c in &parsed.cleaners {
            if !seen.insert(c.id.clone()) {
                return Err(err(format!("duplicate cleaner id '{}'", c.id)));
            }
            if c.targets.is_empty() {
                return Err(err(format!("cleaner '{}' has no target", c.id)));
            }
            if c.privacy && c.risk == Risk::Safe {
                return Err(err(format!("privacy cleaner '{}' cannot be risk = \"safe\"", c.id)));
            }
            for t in &c.targets {
                if t.kind == TargetKind::Files && t.path.trim().is_empty() {
                    return Err(err(format!("cleaner '{}' has a target without path", c.id)));
                }
                for g in t.include.iter().chain(&t.exclude) {
                    globset::Glob::new(g).map_err(|e| err(format!("cleaner '{}': bad glob '{g}': {e}", c.id)))?;
                }
            }
        }
        self.cleaners.extend(parsed.cleaners);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&CleanerDef> {
        self.cleaners.iter().find(|c| c.id == id)
    }

    /// Cleaners that apply to this operating system.
    pub fn for_os(&self, os: &str) -> impl Iterator<Item = &CleanerDef> {
        self.cleaners.iter().filter(move |c| c.os.iter().any(|o| o == os || o == "any"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_rules_parse() {
        let set = RuleSet::builtin();
        assert!(set.cleaners.len() >= 20, "only {} cleaners", set.cleaners.len());
        assert!(set.get("windows.temp.user").is_some());
        for c in &set.cleaners {
            assert!(c.id.contains('.'), "id '{}' should be namespaced", c.id);
            if c.privacy {
                assert!(!c.default_enabled, "privacy cleaner '{}' must be off by default", c.id);
            }
        }
    }

    #[test]
    fn rejects_bad_rules() {
        let mut set = RuleSet::default();
        let dup = r#"
[[cleaner]]
id = "a.b"
name = "x"
description = "x"
category = "system"
risk = "safe"
[[cleaner.target]]
path = "/tmp/x"
"#;
        set.add_file("one.toml", dup).unwrap();
        assert!(set.add_file("two.toml", dup).is_err());

        let privacy_safe = dup.replace("a.b", "c.d").replace("risk = \"safe\"", "risk = \"safe\"\nprivacy = true");
        assert!(set.add_file("p.toml", &privacy_safe).is_err());

        let unknown = dup.replace("a.b", "e.f").replace("path =", "pth =");
        assert!(set.add_file("u.toml", &unknown).is_err());
    }
}
