use std::path::PathBuf;

use crate::{Error, Result};

/// Where Sweeplark keeps its own data (settings, change ledger).
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl AppPaths {
    /// Portable mode: the exe is named `*portable*` or has a `portable.txt` next to it.
    /// Data then lives in `SweeplarkData` beside the exe.
    pub fn portable_home() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let name = exe.file_stem()?.to_string_lossy().to_lowercase();
        (name.contains("portable") || dir.join("portable.txt").exists()).then(|| dir.join("SweeplarkData"))
    }

    pub fn is_portable() -> bool {
        Self::portable_home().is_some()
    }

    /// Installed: `%LOCALAPPDATA%\Sweeplark` (data, logs, AI models) and `%APPDATA%\Sweeplark`
    /// (settings). `SWEEPLARK_HOME` overrides both (tests); portable mode keeps all next to the exe.
    pub fn discover() -> Result<Self> {
        if let Some(home) = std::env::var_os("SWEEPLARK_HOME").map(PathBuf::from).or_else(Self::portable_home) {
            return Ok(Self { data_dir: home.join("data"), config_dir: home.join("config") });
        }
        let dirs = directories::ProjectDirs::from("", "", "Sweeplark")
            .ok_or_else(|| Error::Other("could not find the user profile folder".into()))?;
        Ok(Self { data_dir: dirs.data_local_dir().to_path_buf(), config_dir: dirs.config_dir().to_path_buf() })
    }

    pub fn ledger_db(&self) -> PathBuf {
        self.data_dir.join("ledger.db")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.config_dir)?;
        Ok(())
    }
}

/// Expand `%VAR%` (Windows style) and a leading `~` using `lookup`.
/// Returns `None` if any variable is unknown, so the rule is skipped instead of
/// accidentally pointing at a wrong folder.
pub fn expand_vars(input: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    if let Some(stripped) = rest.strip_prefix('~') {
        out.push_str(&lookup("USERPROFILE").or_else(|| lookup("HOME"))?);
        rest = stripped;
    }
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let name = &after[..end];
        if name.is_empty() {
            return None;
        }
        let value = lookup(name).filter(|v| !v.trim().is_empty())?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Lookup in the real process environment (case-insensitive on Windows by the OS).
pub fn env_lookup(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(name: &str) -> Option<String> {
        match name {
            "LOCALAPPDATA" => Some(r"C:\Users\amit\AppData\Local".into()),
            "USERPROFILE" => Some(r"C:\Users\amit".into()),
            "EMPTY" => Some("  ".into()),
            _ => None,
        }
    }

    #[test]
    fn expands_known_vars() {
        assert_eq!(
            expand_vars("%LOCALAPPDATA%/Temp", &fake).as_deref(),
            Some(r"C:\Users\amit\AppData\Local/Temp")
        );
        assert_eq!(expand_vars("~/x", &fake).as_deref(), Some(r"C:\Users\amit/x"));
    }

    #[test]
    fn unknown_or_empty_var_skips_rule() {
        assert_eq!(expand_vars("%NOPE%/Temp", &fake), None);
        assert_eq!(expand_vars("%EMPTY%/Temp", &fake), None);
        assert_eq!(expand_vars("%LOCALAPPDATA/Temp", &fake), None);
    }
}
