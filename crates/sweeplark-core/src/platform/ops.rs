//! System changes that are not plain registry writes: scheduled tasks, power plans,
//! hibernation, and running Windows tools. Each read has a matching undo.

use std::process::{Command, Output};

use crate::{Error, Result};

/// Full path of a built-in Windows program (`%SystemRoot%\System32\...`), so a file with
/// the same name in the current folder or on PATH is never started instead.
pub fn system_exe(program: &str) -> std::path::PathBuf {
    #[cfg(windows)]
    if !program.contains(['\\', '/']) {
        let root = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| r"C:\Windows".into());
        let name = if program.to_lowercase().ends_with(".exe") { program.to_string() } else { format!("{program}.exe") };
        let lower = name.to_lowercase();
        let candidates = match lower.as_str() {
            "powershell.exe" => vec![root.join(r"System32\WindowsPowerShell\v1.0").join(&name)],
            "explorer.exe" => vec![root.join(&name)],
            _ => vec![root.join("System32").join(&name), root.join(&name)],
        };
        if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
            return found;
        }
    }
    std::path::PathBuf::from(program)
}

/// A `Command` for a built-in Windows program, without a console window.
pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(system_exe(program));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// Run a Windows command without flashing a console window. Gives up (and ends it) after
/// 10 minutes, so a stuck tool can never hold the app forever.
pub fn run(program: &str, args: &[&str]) -> Result<Output> {
    run_timeout(program, args, std::time::Duration::from_secs(600))
}

pub fn run_timeout(program: &str, args: &[&str], limit: std::time::Duration) -> Result<Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut cmd = Command::new(system_exe(program));
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| Error::Other(format!("could not run {program}: {e}")))?;
    let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
    let t_out = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut b);
        }
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_end(&mut b);
        }
        b
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| Error::Other(e.to_string()))? {
            break st;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Other(format!("{program} did not finish in time")));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    Ok(Output { status, stdout: t_out.join().unwrap_or_default(), stderr: t_err.join().unwrap_or_default() })
}

fn ok(out: Output, what: &str) -> Result<String> {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() { text.trim().to_string() } else { err };
        if msg.to_lowercase().contains("access is denied") {
            Err(Error::NeedsAdmin(what.into()))
        } else {
            Err(Error::Other(format!("{what}: {msg}")))
        }
    }
}

/// Decode command output that may be UTF-16 (with BOM) or UTF-8/ANSI.
pub fn decode_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let wide: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&wide)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Whether a task definition (XML) is enabled. Reads `<Settings><Enabled>`, which does not
/// depend on the Windows display language (the CSV "Status" column does).
pub fn task_xml_enabled(xml: &str) -> bool {
    let lower = xml.to_lowercase();
    let settings = match (lower.find("<settings>"), lower.find("</settings>")) {
        (Some(a), Some(b)) if b > a => &lower[a..b],
        _ => return true,
    };
    // Nested elements (idle settings ...) have no <Enabled>, so the first one is the task's.
    !settings.contains("<enabled>false</enabled>")
}

/// `Some(true)` = task enabled, `None` = task not found.
pub fn task_enabled(path: &str) -> Option<bool> {
    let out = run("schtasks", &["/Query", "/TN", path, "/XML"]).ok()?;
    if !out.status.success() {
        return None;
    }
    Some(task_xml_enabled(&decode_output(&out.stdout)))
}

pub fn set_task(path: &str, enabled: bool) -> Result<()> {
    let out = run("schtasks", &["/Change", "/TN", path, if enabled { "/Enable" } else { "/Disable" }])?;
    ok(out, &format!("scheduled task {path}")).map(|_| ())
}

/// GUID of the active power plan.
pub fn active_power_plan() -> Option<String> {
    let out = run("powercfg", &["/getactivescheme"]).ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().find(|w| w.len() == 36 && w.matches('-').count() == 4).map(str::to_lowercase)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

pub fn power_plans() -> Vec<PowerPlan> {
    let Ok(out) = run("powercfg", &["/list"]) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let guid = l.split_whitespace().find(|w| w.len() == 36 && w.matches('-').count() == 4)?;
            let name = l.split('(').nth(1).and_then(|r| r.split(')').next()).unwrap_or(guid).to_string();
            Some(PowerPlan { guid: guid.to_lowercase(), name, active: l.trim_end().ends_with('*') })
        })
        .collect()
}

pub fn set_power_plan(guid: &str) -> Result<()> {
    ok(run("powercfg", &["/setactive", guid])?, "power plan").map(|_| ())
}

/// Add Windows' hidden "Ultimate Performance" plan (once); returns its GUID.
pub fn add_ultimate_plan() -> Result<String> {
    if let Some(p) = power_plans().into_iter().find(is_ultimate) {
        return Ok(p.guid);
    }
    let text = ok(run("powercfg", &["-duplicatescheme", ULTIMATE_BASE])?, "power plan")?;
    let guid = text
        .split_whitespace()
        .find(|w| w.len() == 36 && w.matches('-').count() == 4)
        .map(str::to_lowercase)
        .ok_or_else(|| Error::Other("Ultimate Performance plan was not created".into()))?;
    let reg = crate::platform::registry::system_registry();
    let _ = reg.write_value(crate::platform::registry::Hive::CurrentUser, OWN_KEY, "UltimatePlan", &crate::platform::registry::RegValue::Sz(guid.clone()));
    Ok(guid)
}

/// High performance plan GUID (built into Windows; hidden on some Modern Standby laptops).
pub const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const ULTIMATE_BASE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

const OWN_KEY: &str = r"Software\Sweeplark";

/// GUID of the Ultimate Performance plan Sweeplark created (plan names are translated, so
/// the name alone can't be trusted on non-English Windows).
fn remembered_ultimate() -> Option<String> {
    let reg = crate::platform::registry::system_registry();
    reg.read_value(crate::platform::registry::Hive::CurrentUser, OWN_KEY, "UltimatePlan")
        .ok()
        .flatten()
        .and_then(|v| v.as_string().map(str::to_lowercase))
}

pub fn is_ultimate(p: &PowerPlan) -> bool {
    p.guid == ULTIMATE_BASE || remembered_ultimate().as_deref() == Some(p.guid.as_str()) || p.name.to_lowercase().contains("ultimate")
}

/// The fastest plan on this PC: Ultimate if present, else High performance (added back from
/// Windows' template when the laptop hides it).
pub fn fastest_plan() -> Result<PowerPlan> {
    let plans = power_plans();
    if let Some(p) = plans.iter().find(|p| is_ultimate(p)) {
        return Ok(p.clone());
    }
    if let Some(p) = plans.iter().find(|p| p.guid == HIGH_PERFORMANCE) {
        return Ok(p.clone());
    }
    let text = ok(run("powercfg", &["-duplicatescheme", HIGH_PERFORMANCE])?, "power plan")?;
    let guid = text
        .split_whitespace()
        .find(|w| w.len() == 36 && w.matches('-').count() == 4)
        .map(str::to_lowercase)
        .ok_or_else(|| Error::Other("High performance plan is not available on this PC".into()))?;
    Ok(power_plans().into_iter().find(|p| p.guid == guid).unwrap_or(PowerPlan { guid, name: "High performance".into(), active: false }))
}

/// Whether hibernation (and so Fast Startup) is available.
pub fn hibernate_enabled() -> Option<bool> {
    let reg = crate::platform::registry::system_registry();
    match reg.read_value(crate::platform::registry::Hive::LocalMachine, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabled") {
        Ok(Some(crate::platform::registry::RegValue::Dword(v))) => Some(v != 0),
        _ => None,
    }
}

pub fn set_hibernate(on: bool) -> Result<()> {
    ok(run("powercfg", &["/hibernate", if on { "on" } else { "off" }])?, "hibernation").map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_state_is_language_neutral() {
        let on = "<Task><Triggers><LogonTrigger><Enabled>false</Enabled></LogonTrigger></Triggers><Settings><Priority>7</Priority></Settings></Task>";
        let off = "<Task><Settings><Enabled>false</Enabled></Settings></Task>";
        assert!(task_xml_enabled(on));
        assert!(!task_xml_enabled(off));
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain(off.encode_utf16().flat_map(|c| c.to_le_bytes())).collect();
        assert!(!task_xml_enabled(&decode_output(&utf16)));
    }

    #[test]
    fn parses_guid_lines() {
        let line = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *";
        let guid = line.split_whitespace().find(|w| w.len() == 36 && w.matches('-').count() == 4);
        assert_eq!(guid, Some("381b4222-f694-41f0-9685-ff5bb260df2e"));
    }
}
