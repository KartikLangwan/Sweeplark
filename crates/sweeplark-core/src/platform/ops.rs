//! System changes that are not plain registry writes: scheduled tasks, power plans,
//! hibernation, and running Windows tools. Each read has a matching undo.

use std::process::{Command, Output};

use crate::{Error, Result};

/// Run a Windows command without flashing a console window.
pub fn run(program: &str, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.output().map_err(|e| Error::Other(format!("could not run {program}: {e}")))
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

/// `Some(true)` = task enabled, `None` = task not found.
pub fn task_enabled(path: &str) -> Option<bool> {
    let out = run("schtasks", &["/Query", "/TN", path, "/FO", "CSV", "/NH"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    Some(!text.contains("\"disabled\""))
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

/// Add Windows' hidden "Ultimate Performance" plan; returns its new GUID.
pub fn add_ultimate_plan() -> Result<String> {
    let text = ok(run("powercfg", &["-duplicatescheme", "e9a42b02-d5df-448d-aa00-03f14749eb61"])?, "power plan")?;
    text.split_whitespace()
        .find(|w| w.len() == 36 && w.matches('-').count() == 4)
        .map(str::to_lowercase)
        .ok_or_else(|| Error::Other("Ultimate Performance plan was not created".into()))
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
    #[test]
    fn parses_guid_lines() {
        let line = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *";
        let guid = line.split_whitespace().find(|w| w.len() == 36 && w.matches('-').count() == 4);
        assert_eq!(guid, Some("381b4222-f694-41f0-9685-ff5bb260df2e"));
    }
}
