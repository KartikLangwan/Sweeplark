//! Revert steps: the exact information needed to undo a change.

use serde::{Deserialize, Serialize};

use crate::platform::ops;
use crate::platform::registry::{Hive, RegValue, RegistryBackend};
use crate::Result;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RevertStep {
    /// Put a registry value back the way it was (`previous: None` = the value did not exist).
    RegistryValue { hive: Hive, key: String, name: String, previous: Option<RegValue> },
    /// Put a scheduled task back to enabled/disabled.
    ScheduledTask { path: String, enabled: bool },
    /// Switch back to the power plan that was active before.
    PowerPlan { guid: String },
    /// Turn hibernation back on/off.
    Hibernate { enabled: bool },
    /// Take a process out of Efficiency mode and give back its old priority class.
    /// Skipped when that process has closed (or the id now belongs to another program).
    ProcessPriority { pid: u32, name: String, priority: u32 },
}

impl RevertStep {
    pub fn describe(&self) -> String {
        match self {
            RevertStep::RegistryValue { hive, key, name, previous } => match previous {
                Some(_) => format!("restore {hive:?}\\{key}\\{name}"),
                None => format!("remove {hive:?}\\{key}\\{name}"),
            },
            RevertStep::ScheduledTask { path, enabled } => format!("{} task {path}", if *enabled { "enable" } else { "disable" }),
            RevertStep::PowerPlan { guid } => format!("power plan {guid}"),
            RevertStep::Hibernate { enabled } => format!("hibernation {}", if *enabled { "on" } else { "off" }),
            RevertStep::ProcessPriority { name, .. } => format!("{name}: normal mode"),
        }
    }

    pub fn apply(&self, reg: &dyn RegistryBackend) -> Result<()> {
        match self {
            RevertStep::RegistryValue { hive, key, name, previous } => match previous {
                Some(value) => reg.write_value(*hive, key, name, value),
                None => reg.delete_value(*hive, key, name),
            },
            RevertStep::ScheduledTask { path, enabled } => ops::set_task(path, *enabled),
            RevertStep::PowerPlan { guid } => ops::set_power_plan(guid),
            RevertStep::Hibernate { enabled } => ops::set_hibernate(*enabled),
            RevertStep::ProcessPriority { pid, name, priority } => {
                let same = crate::platform::system::process_name(*pid).is_some_and(|n| {
                    let n = n.to_lowercase();
                    let want = name.to_lowercase();
                    n == want || n.trim_end_matches(".exe") == want.trim_end_matches(".exe")
                });
                if same {
                    let _ = crate::platform::system::set_efficiency_mode(*pid, false);
                    let _ = crate::platform::system::set_priority_class(*pid, *priority);
                }
                Ok(())
            }
        }
    }
}

/// Undo a list of steps, newest first. Every step is tried even when one fails, so one
/// stubborn value does not leave the rest changed; the first error is returned.
pub fn apply_all(steps: &[RevertStep], reg: &dyn RegistryBackend) -> Result<()> {
    let mut first_err = None;
    for step in steps.iter().rev() {
        if let Err(e) = step.apply(reg) {
            crate::log::warn(&format!("undo step failed ({}): {e}", step.describe()));
            first_err.get_or_insert(e);
        }
    }
    first_err.map_or(Ok(()), Err)
}
