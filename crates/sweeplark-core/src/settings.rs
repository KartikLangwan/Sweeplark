use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ai::catalog::Tier;
use crate::{Lang, Result, TrustLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub trust_level: TrustLevel,
    /// Folders/files the cleaner must never touch.
    pub whitelist: Vec<PathBuf>,
    /// Only empty recycle-bin items older than this many days.
    pub recycle_bin_min_age_days: u32,
    /// Browser history/cookies cleaners stay off until the user agrees once.
    pub privacy_consent: bool,
    /// Cleaner ids the user turned off.
    pub disabled_cleaners: Vec<String>,
    /// Cleaner ids (normally off by default) the user turned on.
    pub enabled_cleaners: Vec<String>,
    pub language: Lang,
    pub agent: AgentSettings,
    pub ai: AiSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// Ask before every cleanup (default).
    #[default]
    AskAlways,
    /// Clean safe junk by itself; ask only for risky items.
    RiskyOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Off,
    #[default]
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklySlot {
    /// 0 = Monday ... 6 = Sunday
    pub weekday: u8,
    /// "HH:MM", local time
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoCleanSettings {
    pub frequency: Frequency,
    pub time: String,
    pub weekday: u8,
    /// Wait until nobody has used the PC for a few minutes.
    pub only_when_idle: bool,
    /// Also run when the system drive gets nearly full.
    pub when_disk_low: bool,
}

impl Default for AutoCleanSettings {
    fn default() -> Self {
        Self { frequency: Frequency::Daily, time: "13:00".into(), weekday: 6, only_when_idle: true, when_disk_low: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifySettings {
    pub enabled: bool,
    pub sound: bool,
    pub max_per_day: u32,
    /// No pop-ups between these times ("HH:MM", local). Messages still go to the inbox.
    pub quiet_from: String,
    pub quiet_to: String,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self { enabled: true, sound: true, max_per_day: 4, quiet_from: "22:30".into(), quiet_to: "08:00".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    /// Background monitor (no AI, almost no CPU).
    pub enabled: bool,
    pub start_with_windows: bool,
    pub sample_minutes: u32,
    pub daily_summary: Option<String>,
    pub weekly_summary: Option<WeeklySlot>,
    pub smart_alerts: bool,
    pub auto_clean: AutoCleanSettings,
    pub approval: ApprovalMode,
    pub notify: NotifySettings,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            start_with_windows: true,
            sample_minutes: 5,
            daily_summary: Some("21:00".into()),
            weekly_summary: Some(WeeklySlot { weekday: 6, time: "20:00".into() }),
            smart_alerts: true,
            auto_clean: AutoCleanSettings::default(),
            approval: ApprovalMode::AskAlways,
            notify: NotifySettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    /// Turned on by the user after downloading a model.
    pub enabled: bool,
    pub tier: Option<Tier>,
    pub use_gpu: bool,
    /// Unload the model after this many idle seconds in chat.
    pub unload_after_secs: u64,
    /// Ask the AI about unclear files during automatic cleanup.
    pub review_files: bool,
    /// Let the AI write the daily/weekly summary text.
    pub write_summaries: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self { enabled: false, tier: None, use_gpu: true, unload_after_secs: 120, review_files: true, write_summaries: true }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            trust_level: TrustLevel::Ask,
            whitelist: Vec::new(),
            recycle_bin_min_age_days: 7,
            privacy_consent: false,
            disabled_cleaners: Vec::new(),
            enabled_cleaners: Vec::new(),
            language: Lang::En,
            agent: AgentSettings::default(),
            ai: AiSettings::default(),
        }
    }
}

impl Settings {
    /// Load settings; a missing or broken file gives defaults (never blocks the app).
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert_eq!(Settings::load(&p).recycle_bin_min_age_days, 7);
        let s = Settings { recycle_bin_min_age_days: 30, ..Default::default() };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).recycle_bin_min_age_days, 30);
        std::fs::write(&p, "{ broken").unwrap();
        assert_eq!(Settings::load(&p).recycle_bin_min_age_days, 7);
    }
}
