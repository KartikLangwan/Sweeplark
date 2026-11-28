//! Installed apps (uninstall with the app's own uninstaller, then look for leftovers) and
//! app updates through winget. Nothing here runs without a click.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::platform::registry::{Hive, RegValue, RegistryBackend};
use crate::{Error, Result};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Debug, Clone, Serialize)]
pub struct InstalledApp {
    /// "<hive>:<view>:<subkey>", used to find the entry again.
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub size_bytes: Option<u64>,
    /// YYYYMMDD as Windows stores it.
    pub install_date: Option<String>,
    pub location: Option<String>,
    pub for_all_users: bool,
    #[serde(skip)]
    uninstall: String,
}

fn text(reg: &dyn RegistryBackend, hive: Hive, key: &str, name: &str) -> Option<String> {
    reg.read_value(hive, key, name).ok().flatten().and_then(|v| v.as_string().map(|s| s.trim().to_string())).filter(|s| !s.is_empty())
}

fn dword(reg: &dyn RegistryBackend, hive: Hive, key: &str, name: &str) -> Option<u32> {
    match reg.read_value(hive, key, name).ok().flatten() {
        Some(RegValue::Dword(d)) => Some(d),
        _ => None,
    }
}

/// Apps from the three "Programs and Features" registry lists, sorted by name.
pub fn installed(reg: &dyn RegistryBackend) -> Vec<InstalledApp> {
    let mut out: Vec<InstalledApp> = Vec::new();
    for (hive, base, view) in
        [(Hive::LocalMachine, UNINSTALL, "64"), (Hive::LocalMachine, UNINSTALL32, "32"), (Hive::CurrentUser, UNINSTALL, "u")]
    {
        for sub in reg.list_subkeys(hive, base).unwrap_or_default() {
            let key = format!(r"{base}\{sub}");
            let Some(name) = text(reg, hive, &key, "DisplayName") else { continue };
            // The normal (not "quiet") uninstaller, so the user sees what it removes.
            let Some(uninstall) = text(reg, hive, &key, "UninstallString") else { continue };
            // Updates, runtimes' sub-parts and Windows components hide themselves this way.
            if dword(reg, hive, &key, "SystemComponent") == Some(1) || text(reg, hive, &key, "ParentKeyName").is_some() {
                continue;
            }
            if out.iter().any(|a| a.name == name) {
                continue;
            }
            out.push(InstalledApp {
                id: format!("{}:{view}:{sub}", if hive == Hive::LocalMachine { "m" } else { "u" }),
                publisher: text(reg, hive, &key, "Publisher"),
                version: text(reg, hive, &key, "DisplayVersion"),
                size_bytes: dword(reg, hive, &key, "EstimatedSize").map(|kb| u64::from(kb) * 1024),
                install_date: text(reg, hive, &key, "InstallDate"),
                location: text(reg, hive, &key, "InstallLocation"),
                for_all_users: hive == Hive::LocalMachine,
                name,
                uninstall,
            });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Start the app's own uninstaller (the user sees and controls it).
pub fn uninstall(app: &InstalledApp) -> Result<()> {
    let mut cmd = app.uninstall.clone();
    // "MsiExec.exe /I{GUID}" opens the repair/modify dialog; /X removes.
    let lower = cmd.to_lowercase();
    if lower.contains("msiexec") {
        cmd = cmd.replace("/I{", "/X{").replace("/i{", "/X{");
    }
    crate::log::info(&format!("uninstall started: {}", app.name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The registry string is a full command line; let cmd parse it like Windows does.
        std::process::Command::new(crate::platform::ops::system_exe("cmd"))
            .raw_arg(format!("/c \"{cmd}\""))
            .creation_flags(0x0800_0000)
            .spawn()
            .map(|_| ())
            .map_err(|e| Error::Other(format!("could not start the uninstaller: {e}")))
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
        Err(Error::NotSupported("uninstall".into()))
    }
}

/// Folders an uninstalled app commonly leaves behind: its install folder and folders named
/// exactly like the app (or its publisher) in AppData / ProgramData / Program Files.
/// Only call this after the app is gone from the installed list. The user picks what goes,
/// and it goes to the Recycle Bin.
pub fn leftovers(app: &InstalledApp) -> Vec<String> {
    let mut found: Vec<PathBuf> = Vec::new();
    if let Some(loc) = app.location.as_deref().map(|l| PathBuf::from(l.trim_matches('"')))
        && loc.is_dir()
        && crate::storage::recycle_refusal(&loc).is_none()
    {
        found.push(loc);
    }
    let clean = |s: &str| s.trim().trim_end_matches('.').to_string();
    let mut names = vec![clean(&app.name)];
    // "Foo 2.3 (64-bit)" -> also "Foo"
    if let Some(first) = app.name.split([' ', '(']).next()
        && first.len() >= 4
    {
        names.push(clean(first));
    }
    let publisher = app.publisher.as_deref().map(clean);
    let roots: Vec<PathBuf> =
        ["APPDATA", "LOCALAPPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    for root in &roots {
        let mut candidates: Vec<PathBuf> = names.iter().map(|n| root.join(n)).collect();
        if let Some(p) = &publisher {
            candidates.extend(names.iter().map(|n| root.join(p).join(n)));
        }
        for c in candidates {
            if c.is_dir() && !found.contains(&c) {
                found.push(c);
            }
        }
    }
    found.into_iter().map(|p| p.display().to_string()).collect()
}

/// Is this app still installed (its registry entry exists)?
pub fn still_installed(reg: &dyn RegistryBackend, id: &str) -> bool {
    installed(reg).iter().any(|a| a.id == id)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AppUpdate {
    pub name: String,
    pub id: String,
    pub current: String,
    pub available: String,
}

/// Parse `winget upgrade` table output. Columns are found from the header line above the
/// dashed line (their order is the same in every language: name, id, version, available, source).
pub fn parse_winget(output: &str) -> Vec<AppUpdate> {
    // Progress spinners use carriage returns; keep what was written last on each line.
    let lines: Vec<Vec<char>> = output.lines().map(|l| l.rsplit('\r').next().unwrap_or("").chars().collect()).collect();
    let Some(dash) = lines.iter().position(|l| l.len() > 20 && l.iter().all(|c| *c == '-' || *c == '─')) else {
        return Vec::new();
    };
    let Some(header) = dash.checked_sub(1).map(|i| &lines[i]) else { return Vec::new() };
    let mut starts: Vec<usize> = Vec::new();
    for (i, c) in header.iter().enumerate() {
        if !c.is_whitespace() && (i == 0 || header[i - 1].is_whitespace()) {
            starts.push(i);
        }
    }
    if starts.len() < 4 {
        return Vec::new();
    }
    let col = |l: &[char], k: usize| -> String {
        let a = starts[k].min(l.len());
        let b = starts.get(k + 1).copied().unwrap_or(l.len()).min(l.len());
        l[a..b].iter().collect::<String>().trim().to_string()
    };
    let mut out = Vec::new();
    for l in &lines[dash + 1..] {
        if l.iter().all(|c| c.is_whitespace()) || l.len() < starts[3] {
            break;
        }
        let id = col(l, 1);
        if id.is_empty() || id.contains(' ') || !id.contains('.') {
            continue;
        }
        out.push(AppUpdate { name: col(l, 0), id, current: col(l, 2), available: col(l, 3) });
    }
    out
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c))
}

/// Apps with a newer version in winget. Errors when winget is not installed.
pub fn updates() -> Result<Vec<AppUpdate>> {
    let out = std::process::Command::new("winget")
        .args(["upgrade", "--accept-source-agreements", "--disable-interactivity"])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|_| Error::NotFound("winget (App Installer from the Microsoft Store)".into()))?;
    Ok(parse_winget(&String::from_utf8_lossy(&out.stdout)))
}

/// Update one app (or all with `None`) in a visible window, so the user sees progress.
pub fn update(id: Option<&str>) -> Result<()> {
    let target = match id {
        Some(id) if valid_id(id) => format!("--id {id} -e"),
        Some(_) => return Err(Error::Other("bad app id".into())),
        None => "--all".into(),
    };
    let line = format!("winget upgrade {target} --accept-source-agreements --accept-package-agreements & echo. & pause");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(crate::platform::ops::system_exe("cmd"))
            .raw_arg(format!("/c {line}"))
            .creation_flags(0x0000_0010) // CREATE_NEW_CONSOLE
            .spawn()
            .map(|_| ())
            .map_err(|e| Error::Other(e.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = line;
        Err(Error::NotSupported("winget".into()))
    }
}

/// Folder exists and may be removed (used before sending leftovers to the Recycle Bin).
pub fn is_removable_leftover(path: &Path) -> bool {
    path.is_dir() && crate::storage::recycle_refusal(path).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::registry::MemRegistry;

    #[test]
    fn reads_installed_apps() {
        let reg = MemRegistry::default();
        let k = format!(r"{UNINSTALL}\Foo");
        reg.write_value(Hive::LocalMachine, &k, "DisplayName", &RegValue::Sz("Foo Editor".into())).unwrap();
        reg.write_value(Hive::LocalMachine, &k, "UninstallString", &RegValue::Sz(r#""C:\Foo\unins.exe""#.into())).unwrap();
        reg.write_value(Hive::LocalMachine, &k, "EstimatedSize", &RegValue::Dword(2048)).unwrap();
        let hidden = format!(r"{UNINSTALL}\KB123");
        reg.write_value(Hive::LocalMachine, &hidden, "DisplayName", &RegValue::Sz("Update".into())).unwrap();
        reg.write_value(Hive::LocalMachine, &hidden, "UninstallString", &RegValue::Sz("x".into())).unwrap();
        reg.write_value(Hive::LocalMachine, &hidden, "SystemComponent", &RegValue::Dword(1)).unwrap();
        let u = format!(r"{UNINSTALL}\Bar");
        reg.write_value(Hive::CurrentUser, &u, "DisplayName", &RegValue::Sz("bar app".into())).unwrap();
        reg.write_value(Hive::CurrentUser, &u, "UninstallString", &RegValue::Sz("bar.exe /uninstall".into())).unwrap();
        let apps = installed(&reg);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "bar app");
        assert!(!apps[0].for_all_users);
        assert_eq!(apps[1].size_bytes, Some(2048 * 1024));
        assert!(still_installed(&reg, &apps[1].id));
    }

    #[test]
    fn parses_winget_table() {
        let out = "   - \\ | \r                                                    \r\
Name                       Id                       Version      Available    Source\n\
-------------------------------------------------------------------------------------\n\
Mozilla Firefox (x64 en-US) Mozilla.Firefox          120.0        121.0        winget\n\
7-Zip 22.01 (x64)          7zip.7zip                22.01        23.01        winget\n\
2 upgrades available.\n";
        let u = parse_winget(out);
        assert_eq!(u.len(), 2);
        assert_eq!(u[1], AppUpdate { name: "7-Zip 22.01 (x64)".into(), id: "7zip.7zip".into(), current: "22.01".into(), available: "23.01".into() });
        assert!(parse_winget("No installed package found").is_empty());
        assert!(valid_id("Mozilla.Firefox") && !valid_id("a & calc"));
    }
}
