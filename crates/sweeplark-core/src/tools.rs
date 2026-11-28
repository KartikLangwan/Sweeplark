//! One-click Windows tools: repairs, network fixes, shutdown timer, hardware details.

use serde::Serialize;

use crate::platform::ops::{self, run};
use crate::{Error, Result};

/// Run a command in a visible, elevated console (the user sees progress and the UAC prompt).
fn elevated_console(script: &str) -> Result<()> {
    let ps = format!(
        "Start-Process -FilePath $env:ComSpec -Verb RunAs -ArgumentList '/k {}'",
        script.replace('\'', "''")
    );
    let out = run("powershell", &["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])?;
    if out.status.success() { Ok(()) } else { Err(Error::Other("cancelled".into())) }
}

fn system_drive() -> String {
    std::env::var("SystemDrive").ok().filter(|d| d.len() == 2 && d.ends_with(':')).unwrap_or_else(|| "C:".into())
}

fn restart_explorer() -> Result<()> {
    let _ = run("taskkill", &["/F", "/IM", "explorer.exe"]);
    std::thread::sleep(std::time::Duration::from_millis(800));
    std::process::Command::new(ops::system_exe("explorer.exe")).spawn().map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

/// Create a Windows restore point (needs administrator). Windows normally allows one per
/// 24 hours; the limit is lifted for this one call and put back right after.
pub fn create_restore_point(description: &str) -> Result<()> {
    if !crate::platform::system::is_admin() {
        return Err(Error::NeedsAdmin("restore point".into()));
    }
    let desc: String = description.chars().filter(|c| c.is_alphanumeric() || " .:-_()".contains(*c)).take(60).collect();
    let script = format!(
        "$ErrorActionPreference='Stop'; $k='HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore'; \
         $old=(Get-ItemProperty $k -Name SystemRestorePointCreationFrequency -ErrorAction SilentlyContinue).SystemRestorePointCreationFrequency; \
         Set-ItemProperty $k -Name SystemRestorePointCreationFrequency -Value 0 -Type DWord; \
         try {{ Checkpoint-Computer -Description 'Sweeplark: {desc}' -RestorePointType MODIFY_SETTINGS }} \
         finally {{ if ($null -eq $old) {{ Remove-ItemProperty $k -Name SystemRestorePointCreationFrequency -ErrorAction SilentlyContinue }} \
         else {{ Set-ItemProperty $k -Name SystemRestorePointCreationFrequency -Value $old -Type DWord }} }}"
    );
    let out = run("powershell", &["-NoProfile", "-NonInteractive", "-Command", &script])?;
    if out.status.success() {
        crate::log::info(&format!("restore point created: {desc}"));
        Ok(())
    } else {
        let msg = ops::decode_output(&out.stderr);
        crate::log::warn(&format!("restore point failed: {}", msg.trim()));
        Err(Error::Other(
            "Could not create a restore point. System Protection may be off for the system drive (Control Panel > System > System Protection).".into(),
        ))
    }
}

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    FlushDns,
    RestartExplorer,
    RebuildIconCache,
    Sfc,
    Dism,
    ResetNetwork,
    ResetWindowsUpdate,
    DiskCheck,
    CancelShutdown,
    ComponentCleanup,
    RestorePoint,
}

pub fn run_tool(tool: Tool) -> Result<String> {
    match tool {
        Tool::FlushDns => {
            let out = run("ipconfig", &["/flushdns"])?;
            if out.status.success() { Ok("DNS cache cleared".into()) } else { Err(Error::Other("ipconfig failed".into())) }
        }
        Tool::RestartExplorer => restart_explorer().map(|_| "Explorer restarted".into()),
        Tool::RebuildIconCache => {
            let _ = run("ie4uinit.exe", &["-show"]);
            let local = std::env::var("LOCALAPPDATA").map_err(|_| Error::NotFound("LOCALAPPDATA".into()))?;
            let dir = std::path::Path::new(&local).join(r"Microsoft\Windows\Explorer");
            let _ = run("taskkill", &["/F", "/IM", "explorer.exe"]);
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if n.starts_with("iconcache") && n.ends_with(".db") {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            let _ = std::fs::remove_file(std::path::Path::new(&local).join("IconCache.db"));
            // Always bring the desktop back, even if a file could not be removed.
            std::process::Command::new(ops::system_exe("explorer.exe")).spawn().map_err(|e| Error::Other(e.to_string()))?;
            Ok("Icon cache rebuilt".into())
        }
        Tool::Sfc => elevated_console("sfc /scannow").map(|_| "System file check started".into()),
        Tool::Dism => elevated_console("DISM /Online /Cleanup-Image /RestoreHealth").map(|_| "Windows image repair started".into()),
        Tool::DiskCheck => elevated_console(&format!("chkdsk {} /scan", system_drive())).map(|_| "Disk check started".into()),
        Tool::ComponentCleanup => elevated_console("DISM /Online /Cleanup-Image /StartComponentCleanup && echo. && echo Done. Old Windows update files were removed.")
            .map(|_| "Windows update cleanup started".into()),
        // Not admin: ask Windows (UAC) and show it in its own window, like the other repairs.
        Tool::RestorePoint if !crate::platform::system::is_admin() => elevated_console(
            "powershell -NoProfile -Command \"Checkpoint-Computer -Description 'Sweeplark manual' -RestorePointType MODIFY_SETTINGS\" && echo. && echo Restore point created.",
        )
        .map(|_| "Restore point started".into()),
        Tool::RestorePoint => create_restore_point("manual").map(|_| "Restore point created".into()),
        Tool::ResetNetwork => elevated_console(
            // "&": every step runs (ipconfig /release fails on fixed-IP PCs; the rest still matters).
            "netsh winsock reset & netsh int ip reset & ipconfig /release & ipconfig /renew & ipconfig /flushdns & echo. & echo Restart the PC to finish.",
        )
        .map(|_| "Network reset started".into()),
        // Every step runs even if one fails ("&"), so the services are always started again,
        // and an old backup from an earlier run is removed first so "ren" cannot fail.
        Tool::ResetWindowsUpdate => elevated_console(concat!(
            "net stop wuauserv /y & net stop bits /y & net stop cryptsvc /y",
            " & (if exist \"%SystemRoot%\\SoftwareDistribution.old\" rd /s /q \"%SystemRoot%\\SoftwareDistribution.old\")",
            " & ren \"%SystemRoot%\\SoftwareDistribution\" SoftwareDistribution.old",
            " & (if exist \"%SystemRoot%\\System32\\catroot2.old\" rd /s /q \"%SystemRoot%\\System32\\catroot2.old\")",
            " & ren \"%SystemRoot%\\System32\\catroot2\" catroot2.old",
            " & net start cryptsvc & net start bits & net start wuauserv",
            " & echo. & echo Windows Update components reset. Restart the PC if updates still fail."
        ))
        .map(|_| "Windows Update reset started".into()),
        Tool::CancelShutdown => {
            let _ = run("shutdown", &["/a"]);
            Ok("Scheduled shutdown cancelled".into())
        }
    }
}

/// Shut down (or restart) after `minutes`.
pub fn schedule_shutdown(minutes: u32, restart: bool) -> Result<()> {
    let secs = (minutes.clamp(1, 24 * 60) * 60).to_string();
    let out = run("shutdown", &[if restart { "/r" } else { "/s" }, "/t", &secs])?;
    if out.status.success() { Ok(()) } else { Err(Error::Other("shutdown command failed".into())) }
}

/// Open a built-in Windows tool (allow-list only).
pub fn open_windows_tool(name: &str) -> Result<()> {
    let (prog, args): (&str, &[&str]) = match name {
        "task_manager" => ("taskmgr", &[]),
        "device_manager" => ("mmc", &["devmgmt.msc"]),
        "services" => ("mmc", &["services.msc"]),
        "event_viewer" => ("mmc", &["eventvwr.msc"]),
        "disk_management" => ("mmc", &["diskmgmt.msc"]),
        "resource_monitor" => ("resmon", &[]),
        "system_info" => ("msinfo32", &[]),
        "disk_cleanup" => ("cleanmgr", &[]),
        "control_panel" => ("control", &[]),
        "windows_update" => ("explorer", &["ms-settings:windowsupdate"]),
        "apps" => ("explorer", &["ms-settings:appsfeatures"]),
        "storage_sense" => ("explorer", &["ms-settings:storagesense"]),
        "environment" => ("rundll32", &["sysdm.cpl,EditEnvironmentVariables"]),
        _ => return Err(Error::NotFound(name.into())),
    };
    std::process::Command::new(ops::system_exe(prog)).args(args).spawn().map(|_| ()).map_err(|e| Error::Other(e.to_string()))
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HardwareInfo {
    pub items: Vec<(String, String)>,
}

/// Hardware details for the "System info" card.
pub fn hardware_info() -> HardwareInfo {
    let mut items: Vec<(String, String)> = Vec::new();
    let h = crate::health::snapshot(false);
    items.push(("Windows".into(), h.os.clone()));
    items.push(("PC name".into(), h.host_name.clone()));
    items.push(("Processor".into(), format!("{} ({} threads)", h.cpu_brand, h.cpu_cores)));
    items.push(("Memory".into(), crate::util::format_bytes(h.memory_total)));
    for g in &h.gpus {
        let mem = g.memory_bytes.map(|m| format!(" · {}", crate::util::format_bytes(m))).unwrap_or_default();
        items.push(("Graphics".into(), format!("{}{mem}", g.name)));
    }
    for d in h.disks.iter().filter(|d| !d.removable) {
        items.push((format!("Drive {}", d.mount.trim_end_matches('\\')), format!("{} {} · {}", d.kind, crate::util::format_bytes(d.total_bytes), d.file_system)));
    }
    #[cfg(windows)]
    items.extend(win::wmi_details());
    HardwareInfo { items }
}

#[cfg(windows)]
mod win {
    use std::collections::HashMap;

    use wmi::{Variant, WMIConnection};

    fn s(v: Option<&Variant>) -> Option<String> {
        match v? {
            Variant::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Variant::UI4(n) => Some(n.to_string()),
            Variant::UI8(n) => Some(n.to_string()),
            _ => None,
        }
    }

    pub fn wmi_details() -> Vec<(String, String)> {
        let mut out = Vec::new();
        let Ok(con) = WMIConnection::new() else { return out };
        type Row = HashMap<String, Variant>;
        if let Ok(rows) = con.raw_query::<Row>("SELECT Manufacturer, Product FROM Win32_BaseBoard")
            && let Some(r) = rows.first()
        {
            out.push(("Motherboard".into(), format!("{} {}", s(r.get("Manufacturer")).unwrap_or_default(), s(r.get("Product")).unwrap_or_default())));
        }
        if let Ok(rows) = con.raw_query::<Row>("SELECT Manufacturer, SMBIOSBIOSVersion FROM Win32_BIOS")
            && let Some(r) = rows.first()
        {
            out.push(("BIOS".into(), format!("{} {}", s(r.get("Manufacturer")).unwrap_or_default(), s(r.get("SMBIOSBIOSVersion")).unwrap_or_default())));
        }
        if let Ok(rows) = con.raw_query::<Row>("SELECT Capacity, ConfiguredClockSpeed, Manufacturer FROM Win32_PhysicalMemory") {
            let sticks: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cap = s(r.get("Capacity")).and_then(|c| c.parse::<u64>().ok()).map(crate::util::format_bytes).unwrap_or_default();
                    let speed = s(r.get("ConfiguredClockSpeed")).map(|v| format!(" @ {v} MT/s")).unwrap_or_default();
                    format!("{cap}{speed}")
                })
                .collect();
            if !sticks.is_empty() {
                out.push(("RAM sticks".into(), sticks.join(" + ")));
            }
        }
        if let Ok(rows) = con.raw_query::<Row>("SELECT Name, DriverVersion FROM Win32_VideoController") {
            for r in rows {
                if let (Some(n), Some(v)) = (s(r.get("Name")), s(r.get("DriverVersion"))) {
                    out.push(("GPU driver".into(), format!("{n} · {v}")));
                }
            }
        }
        out
    }
}
