//! Startup manager. Entries are switched on/off the same way Task Manager does it
//! (the `StartupApproved` registry keys), so nothing is deleted and every change can be undone.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::ledger::{ChangeStatus, Ledger, NewChange};
use crate::platform::registry::{Hive, RegValue, RegistryBackend};
use crate::revert::RevertStep;
use crate::util::system_time_to_filetime;
use crate::{Error, Result, Risk};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupSource {
    /// HKCU\...\Run
    RunUser,
    /// HKLM\...\Run
    RunMachine,
    /// HKLM\...\WOW6432Node\...\Run (32-bit apps)
    RunMachine32,
    /// Startup folder of this user
    FolderUser,
    /// Startup folder for all users
    FolderCommon,
}

impl StartupSource {
    fn tag(self) -> &'static str {
        match self {
            StartupSource::RunUser => "run-user",
            StartupSource::RunMachine => "run-machine",
            StartupSource::RunMachine32 => "run-machine32",
            StartupSource::FolderUser => "folder-user",
            StartupSource::FolderCommon => "folder-common",
        }
    }

    /// Where Windows stores the on/off state.
    pub fn approved_key(self) -> (Hive, String) {
        match self {
            StartupSource::RunUser => (Hive::CurrentUser, format!(r"{APPROVED}\Run")),
            StartupSource::RunMachine => (Hive::LocalMachine, format!(r"{APPROVED}\Run")),
            StartupSource::RunMachine32 => (Hive::LocalMachine, format!(r"{APPROVED}\Run32")),
            StartupSource::FolderUser => (Hive::CurrentUser, format!(r"{APPROVED}\StartupFolder")),
            StartupSource::FolderCommon => (Hive::LocalMachine, format!(r"{APPROVED}\StartupFolder")),
        }
    }

    pub fn needs_admin(self) -> bool {
        self.approved_key().0.needs_admin_to_write()
    }

    pub fn label(self) -> &'static str {
        match self {
            StartupSource::RunUser => "Registry (sirf aap)",
            StartupSource::RunMachine | StartupSource::RunMachine32 => "Registry (sab users)",
            StartupSource::FolderUser => "Startup folder (sirf aap)",
            StartupSource::FolderCommon => "Startup folder (sab users)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Advice {
    Keep,
    Disable,
    Neutral,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    pub command: String,
    pub source: StartupSource,
    pub source_label: String,
    pub enabled: bool,
    pub needs_admin: bool,
    pub advice: Advice,
    pub advice_note: Option<String>,
    /// Registry value name in the StartupApproved key.
    #[serde(skip)]
    approved_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KnownApp {
    #[serde(rename = "match")]
    pattern: String,
    verdict: Advice,
    note: String,
}

#[derive(Debug, Deserialize)]
struct KnownFile {
    app: Vec<KnownApp>,
}

#[derive(Debug, Clone, Default)]
pub struct KnownApps {
    apps: Vec<KnownApp>,
}

impl KnownApps {
    pub fn builtin() -> Self {
        let text = include_str!("../../../rules/startup/known.toml");
        let parsed: KnownFile = toml::from_str(text).expect("built-in startup rules broken");
        Self { apps: parsed.app }
    }

    fn advise(&self, name: &str, command: &str) -> (Advice, Option<String>) {
        let hay = format!("{name} {command}").to_lowercase();
        self.apps
            .iter()
            .find(|a| hay.contains(&a.pattern.to_lowercase()))
            .map(|a| (a.verdict, Some(a.note.clone())))
            .unwrap_or((Advice::Neutral, None))
    }
}

/// `StartupApproved` values are 12 bytes: a 4-byte flag (2 = on, 3 = off) and the FILETIME
/// when the entry was switched off. A missing value means "on".
fn is_enabled(value: Option<&RegValue>) -> bool {
    match value {
        Some(RegValue::Binary(b)) if !b.is_empty() => b[0] & 1 == 0,
        _ => true,
    }
}

fn approved_value(enabled: bool, now: SystemTime) -> RegValue {
    let mut bytes = vec![0u8; 12];
    if enabled {
        bytes[0] = 2;
    } else {
        bytes[0] = 3;
        bytes[4..12].copy_from_slice(&system_time_to_filetime(now).to_le_bytes());
    }
    RegValue::Binary(bytes)
}

#[derive(Debug, Clone, Default)]
pub struct StartupFolders {
    pub user: Option<PathBuf>,
    pub common: Option<PathBuf>,
}

impl StartupFolders {
    pub fn system() -> Self {
        let sub = r"Microsoft\Windows\Start Menu\Programs\Startup";
        Self {
            user: std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(sub)),
            common: std::env::var_os("ProgramData").map(|p| PathBuf::from(p).join(sub)),
        }
    }
}

fn make_item(
    source: StartupSource,
    name: String,
    command: String,
    approved_name: String,
    reg: &dyn RegistryBackend,
    known: &KnownApps,
) -> Result<StartupItem> {
    let (hive, key) = source.approved_key();
    let state = reg.read_value(hive, &key, &approved_name)?;
    let (advice, advice_note) = known.advise(&name, &command);
    Ok(StartupItem {
        id: format!("{}:{}", source.tag(), approved_name),
        name,
        command,
        source,
        source_label: source.label().into(),
        enabled: is_enabled(state.as_ref()),
        needs_admin: source.needs_admin(),
        advice,
        advice_note,
        approved_name,
    })
}

fn folder_items(
    dir: &Path,
    source: StartupSource,
    reg: &dyn RegistryBackend,
    known: &KnownApps,
    out: &mut Vec<StartupItem>,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.eq_ignore_ascii_case("desktop.ini") || !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(file.clone());
        out.push(make_item(source, name, entry.path().display().to_string(), file, reg, known)?);
    }
    Ok(())
}

/// All startup entries, sorted by name.
pub fn list(reg: &dyn RegistryBackend, folders: &StartupFolders, known: &KnownApps) -> Result<Vec<StartupItem>> {
    let mut items = Vec::new();
    for (source, hive, key) in [
        (StartupSource::RunUser, Hive::CurrentUser, RUN),
        (StartupSource::RunMachine, Hive::LocalMachine, RUN),
        (StartupSource::RunMachine32, Hive::LocalMachine, RUN32),
    ] {
        for (name, value) in reg.list_values(hive, key)? {
            if name.is_empty() {
                continue;
            }
            let command = value.as_string().unwrap_or_default().to_string();
            items.push(make_item(source, name.clone(), command, name, reg, known)?);
        }
    }
    if let Some(dir) = &folders.user {
        folder_items(dir, StartupSource::FolderUser, reg, known, &mut items)?;
    }
    if let Some(dir) = &folders.common {
        folder_items(dir, StartupSource::FolderCommon, reg, known, &mut items)?;
    }
    items.sort_by_key(|i| i.name.to_lowercase());
    Ok(items)
}

/// Switch an entry on or off. Returns the steps that undo it.
pub fn set_enabled(
    reg: &dyn RegistryBackend,
    item: &StartupItem,
    enabled: bool,
    is_admin: bool,
) -> Result<Vec<RevertStep>> {
    if item.needs_admin && !is_admin {
        return Err(Error::NeedsAdmin(format!("'{}' is set up for all users", item.name)));
    }
    let (hive, key) = item.source.approved_key();
    let previous = reg.read_value(hive, &key, &item.approved_name)?;
    reg.write_value(hive, &key, &item.approved_name, &approved_value(enabled, SystemTime::now()))?;
    Ok(vec![RevertStep::RegistryValue { hive, key, name: item.approved_name.clone(), previous }])
}

/// Switch an entry and write it to the ledger. Returns the ledger id.
pub fn set_enabled_recorded(
    reg: &dyn RegistryBackend,
    ledger: &Ledger,
    item: &StartupItem,
    enabled: bool,
    is_admin: bool,
) -> Result<i64> {
    let revert = set_enabled(reg, item, enabled, is_admin)?;
    ledger.record(&NewChange {
        category: "startup".into(),
        title: if enabled {
            format!("Startup mein chalu kiya: {}", item.name)
        } else {
            format!("Startup se band kiya: {}", item.name)
        },
        risk: Risk::Ask,
        status: ChangeStatus::Applied,
        bytes_freed: 0,
        items: 1,
        details: serde_json::json!({ "id": item.id, "name": item.name, "command": item.command, "enabled": enabled }),
        revert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::registry::MemRegistry;

    fn setup() -> (MemRegistry, tempfile::TempDir) {
        let reg = MemRegistry::default();
        reg.write_value(Hive::CurrentUser, RUN, "Spotify", &RegValue::Sz(r#""C:\Spotify.exe" /minimized"#.into()))
            .unwrap();
        reg.write_value(Hive::LocalMachine, RUN, "SecurityHealth", &RegValue::ExpandSz("SecurityHealthSystray.exe".into()))
            .unwrap();
        reg.write_value(Hive::LocalMachine, RUN32, "OldTool", &RegValue::Sz("tool.exe".into())).unwrap();
        // Discord was switched off earlier via Task Manager.
        reg.write_value(Hive::CurrentUser, RUN, "Discord", &RegValue::Sz("Update.exe --processStart Discord.exe".into()))
            .unwrap();
        reg.write_value(Hive::CurrentUser, &format!(r"{APPROVED}\Run"), "Discord", &approved_value(false, SystemTime::now()))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Notes.lnk"), b"").unwrap();
        std::fs::write(dir.path().join("desktop.ini"), b"").unwrap();
        (reg, dir)
    }

    #[test]
    fn lists_entries_with_state_and_advice() {
        let (reg, dir) = setup();
        let folders = StartupFolders { user: Some(dir.path().to_path_buf()), common: None };
        let items = list(&reg, &folders, &KnownApps::builtin()).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Discord", "Notes", "OldTool", "SecurityHealth", "Spotify"]);
        let get = |n: &str| items.iter().find(|i| i.name == n).unwrap();
        assert!(!get("Discord").enabled);
        assert!(get("Spotify").enabled);
        assert_eq!(get("Spotify").advice, Advice::Disable);
        assert_eq!(get("SecurityHealth").advice, Advice::Keep);
        assert!(get("SecurityHealth").needs_admin);
        assert_eq!(get("Notes").source, StartupSource::FolderUser);
        assert_eq!(get("OldTool").source, StartupSource::RunMachine32);
    }

    #[test]
    fn disable_then_undo() {
        let (reg, dir) = setup();
        let folders = StartupFolders { user: Some(dir.path().to_path_buf()), common: None };
        let known = KnownApps::builtin();
        let ledger = Ledger::open_in_memory().unwrap();
        let spotify = list(&reg, &folders, &known).unwrap().into_iter().find(|i| i.name == "Spotify").unwrap();

        let id = set_enabled_recorded(&reg, &ledger, &spotify, false, false).unwrap();
        let after = list(&reg, &folders, &known).unwrap().into_iter().find(|i| i.name == "Spotify").unwrap();
        assert!(!after.enabled);
        // The Run entry itself is untouched.
        assert!(reg.read_value(Hive::CurrentUser, RUN, "Spotify").unwrap().is_some());

        ledger.undo(id, &reg).unwrap();
        let undone = list(&reg, &folders, &known).unwrap().into_iter().find(|i| i.name == "Spotify").unwrap();
        assert!(undone.enabled);
        assert_eq!(reg.read_value(Hive::CurrentUser, &format!(r"{APPROVED}\Run"), "Spotify").unwrap(), None);
    }

    #[test]
    fn machine_entries_need_admin() {
        let (reg, dir) = setup();
        let folders = StartupFolders { user: Some(dir.path().to_path_buf()), common: None };
        let items = list(&reg, &folders, &KnownApps::builtin()).unwrap();
        let tool = items.iter().find(|i| i.name == "OldTool").unwrap();
        assert!(matches!(set_enabled(&reg, tool, false, false), Err(Error::NeedsAdmin(_))));
        assert!(set_enabled(&reg, tool, false, true).is_ok());
    }

    #[test]
    fn approved_bytes() {
        assert!(is_enabled(None));
        assert!(is_enabled(Some(&approved_value(true, SystemTime::now()))));
        assert!(!is_enabled(Some(&approved_value(false, SystemTime::now()))));
        assert!(is_enabled(Some(&RegValue::Binary(vec![6, 0, 0, 0]))));
        assert!(!is_enabled(Some(&RegValue::Binary(vec![7, 0, 0, 0]))));
    }
}
