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
        }
    }
}

/// Undo a list of steps, newest first.
pub fn apply_all(steps: &[RevertStep], reg: &dyn RegistryBackend) -> Result<()> {
    for step in steps.iter().rev() {
        step.apply(reg)?;
    }
    Ok(())
}
