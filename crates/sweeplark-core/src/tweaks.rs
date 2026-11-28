//! Optimizer: Windows tweaks as data (`rules/tweaks/*.toml`).
//!
//! A tweak is a list of registry values, service start types, scheduled tasks and a few
//! power actions. Applying one records the exact previous state in the ledger, so every
//! tweak can be switched off again. Ideas and values follow optimizerDuck (GPL-3.0) and
//! Microsoft documentation.

use serde::{Deserialize, Serialize};

use crate::ledger::{ChangeStatus, Ledger, NewChange};
use crate::platform::ops;
use crate::platform::registry::{Hive, RegValue, RegistryBackend, parse_key};
use crate::revert::RevertStep;
use crate::{Error, Lang, Result, Risk, tr};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegItem {
    /// Full key with hive, e.g. `HKLM\SOFTWARE\Policies\...`. `{gpu:nvidia}` etc. expand to
    /// the display adapter's driver key.
    pub key: String,
    pub name: String,
    /// Integer → DWORD, string → REG_SZ (see `kind`).
    #[serde(default)]
    pub value: Option<toml::Value>,
    /// "qword" or "expand" to override the type, or "flags": a `Key=Value;` list where only
    /// the named keys are changed and other entries are kept (DirectX settings string).
    #[serde(default)]
    pub kind: Option<String>,
    /// Delete the value instead of writing it.
    #[serde(default)]
    pub delete: bool,
    /// Value written when a setting is switched off without an undo record.
    #[serde(default)]
    pub off: Option<toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceItem {
    pub name: String,
    /// "auto", "manual" or "disabled".
    pub start: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskItem {
    pub path: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Performance,
    Gaming,
    Gpu,
    Power,
    Privacy,
    Ai,
    Services,
    Explorer,
    Taskbar,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TweakDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_hi: Option<String>,
    pub description: String,
    #[serde(default)]
    pub description_hi: Option<String>,
    pub category: Category,
    pub risk: Risk,
    /// Part of "Apply recommended".
    #[serde(default)]
    pub recommended: bool,
    /// "win11", "win10", "laptop", "desktop", "nvidia", "amd", "intel_gpu"
    #[serde(default)]
    pub conditions: Vec<String>,
    /// "explorer", "signout" or "reboot"
    #[serde(default)]
    pub restart: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub note_hi: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    /// A two-way setting (on/off) rather than an optimization.
    #[serde(default)]
    pub setting: bool,
    #[serde(default, rename = "registry")]
    pub registry: Vec<RegItem>,
    #[serde(default, rename = "service")]
    pub services: Vec<ServiceItem>,
    #[serde(default, rename = "task")]
    pub tasks: Vec<TaskItem>,
    /// "hibernate_off" | "ultimate_plan"
    #[serde(default)]
    pub action: Option<String>,
}

impl TweakDef {
    pub fn needs_admin(&self) -> bool {
        self.registry.iter().any(|r| {
            let k = r.key.to_ascii_uppercase();
            k.starts_with("HKLM") || k.starts_with("HKEY_LOCAL_MACHINE") || r.key.starts_with("{gpu")
        })
            || !self.services.is_empty()
            || !self.tasks.is_empty()
            || self.action.is_some()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TweakFile {
    tweak: Vec<TweakDef>,
}

const BUILTIN: &[(&str, &str)] = &[
    ("performance.toml", include_str!("../../../rules/tweaks/performance.toml")),
    ("gaming.toml", include_str!("../../../rules/tweaks/gaming.toml")),
    ("gpu.toml", include_str!("../../../rules/tweaks/gpu.toml")),
    ("power.toml", include_str!("../../../rules/tweaks/power.toml")),
    ("privacy.toml", include_str!("../../../rules/tweaks/privacy.toml")),
    ("ai.toml", include_str!("../../../rules/tweaks/ai.toml")),
    ("services.toml", include_str!("../../../rules/tweaks/services.toml")),
    ("explorer.toml", include_str!("../../../rules/tweaks/explorer.toml")),
    ("system.toml", include_str!("../../../rules/tweaks/system.toml")),
];

#[derive(Debug, Clone, Default)]
pub struct TweakSet {
    pub tweaks: Vec<TweakDef>,
}

impl TweakSet {
    pub fn builtin() -> Self {
        let mut set = TweakSet::default();
        for (file, text) in BUILTIN {
            set.add_file(file, text).unwrap_or_else(|e| panic!("built-in tweaks broken: {e}"));
        }
        set
    }

    pub fn add_file(&mut self, file: &str, text: &str) -> Result<()> {
        let err = |m: String| Error::Rules { file: file.into(), message: m };
        let parsed: TweakFile = toml::from_str(text).map_err(|e| err(e.to_string()))?;
        for t in &parsed.tweak {
            if self.tweaks.iter().any(|x| x.id == t.id) {
                return Err(err(format!("duplicate tweak id '{}'", t.id)));
            }
            if t.registry.is_empty() && t.services.is_empty() && t.tasks.is_empty() && t.action.is_none() {
                return Err(err(format!("tweak '{}' does nothing", t.id)));
            }
            for r in &t.registry {
                if !r.key.starts_with("{gpu:") && parse_key(&r.key).is_none() {
                    return Err(err(format!("tweak '{}': bad key '{}'", t.id, r.key)));
                }
                if !r.delete && r.value.is_none() {
                    return Err(err(format!("tweak '{}': value missing for {}", t.id, r.name)));
                }
                for v in [&r.value, &r.off].into_iter().flatten() {
                    let kind = if is_flags(r) { None } else { r.kind.as_deref() };
                    to_reg(v, kind).ok_or_else(|| err(format!("tweak '{}': bad value for {}", t.id, r.name)))?;
                }
            }
            for s in &t.services {
                start_code(&s.start).ok_or_else(|| err(format!("tweak '{}': bad start '{}'", t.id, s.start)))?;
            }
        }
        self.tweaks.extend(parsed.tweak);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&TweakDef> {
        self.tweaks.iter().find(|t| t.id == id)
    }
}

fn to_reg(v: &toml::Value, kind: Option<&str>) -> Option<RegValue> {
    Some(match (v, kind) {
        (toml::Value::Integer(i), Some("qword")) => RegValue::Qword(*i as u64),
        (toml::Value::Integer(i), _) => RegValue::Dword(if *i < 0 { *i as i32 as u32 } else { u32::try_from(*i).ok()? }),
        (toml::Value::String(s), Some("expand")) => RegValue::ExpandSz(s.clone()),
        (toml::Value::String(s), _) => RegValue::Sz(s.clone()),
        _ => return None,
    })
}

/// `Key=Value;` entries of a flags string, in order.
fn flag_pairs(s: &str) -> Vec<(String, String)> {
    s.split(';')
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            let k = k.trim();
            (!k.is_empty()).then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// `current` with the entries of `want` set (others kept).
fn merge_flags(current: &str, want: &str) -> String {
    let mut pairs = flag_pairs(current);
    for (k, v) in flag_pairs(want) {
        match pairs.iter_mut().find(|(pk, _)| pk.eq_ignore_ascii_case(&k)) {
            Some(p) => p.1 = v,
            None => pairs.push((k, v)),
        }
    }
    pairs.iter().map(|(k, v)| format!("{k}={v};")).collect()
}

fn flags_contain(current: &str, want: &str) -> bool {
    let have = flag_pairs(current);
    flag_pairs(want).iter().all(|(k, v)| have.iter().any(|(hk, hv)| hk.eq_ignore_ascii_case(k) && hv == v))
}

fn is_flags(r: &RegItem) -> bool {
    r.kind.as_deref() == Some("flags")
}

/// The value to write for `r` given what is there now (flags merge into the current string).
fn target_value(r: &RegItem, v: &toml::Value, current: Option<&RegValue>) -> Option<RegValue> {
    if is_flags(r) {
        let want = v.as_str()?;
        let cur = current.and_then(|c| c.as_string()).unwrap_or("");
        return Some(RegValue::Sz(merge_flags(cur, want)));
    }
    to_reg(v, r.kind.as_deref())
}

fn start_code(s: &str) -> Option<u32> {
    match s {
        "auto" => Some(2),
        "manual" => Some(3),
        "disabled" => Some(4),
        _ => None,
    }
}

/// Facts about this PC used for tweak conditions.
#[derive(Debug, Clone, Default)]
pub struct TweakContext {
    pub windows_build: u32,
    pub laptop: bool,
    pub gpu_names: Vec<String>,
    pub is_admin: bool,
    pub lang: Lang,
}

impl TweakContext {
    /// Read once and keep: Windows build, laptop or not and graphics cards do not change while
    /// the app runs. Callers refresh `is_admin` and `lang` per call (both are cheap).
    pub fn system(lang: Lang) -> Self {
        let gpu_names = sysinfo::Gpus::new_with_refreshed_list()
            .map(|list| {
                list.iter()
                    .map(|g| [g.vendor(), g.model()].into_iter().flatten().collect::<Vec<_>>().join(" ").to_lowercase())
                    .filter(|n| !n.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            windows_build: sysinfo::System::kernel_version().ok().and_then(|v| v.split('.').next_back()?.parse().ok()).unwrap_or(0),
            laptop: crate::platform::system::battery().is_some(),
            gpu_names,
            is_admin: crate::platform::system::is_admin(),
            lang,
        }
    }

    fn has_gpu(&self, vendor: &str) -> bool {
        self.gpu_names.iter().any(|g| match vendor {
            "nvidia" => g.contains("nvidia"),
            "amd" => g.contains("amd") || g.contains("radeon"),
            "intel" => g.contains("intel"),
            _ => false,
        })
    }

    pub fn allows(&self, t: &TweakDef) -> bool {
        t.conditions.iter().all(|c| match c.as_str() {
            "win11" => self.windows_build == 0 || self.windows_build >= 22000,
            "win10" => self.windows_build == 0 || self.windows_build < 22000,
            "laptop" => self.laptop,
            "desktop" => !self.laptop,
            "nvidia" => self.has_gpu("nvidia"),
            "amd" => self.has_gpu("amd"),
            "intel_gpu" => self.has_gpu("intel"),
            _ => true,
        })
    }
}

const DISPLAY_CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// Expand `{gpu:vendor}` to the driver keys of matching display adapters.
fn resolve_keys(key: &str, reg: &dyn RegistryBackend) -> Vec<(Hive, String)> {
    let Some(vendor) = key.strip_prefix("{gpu:").and_then(|r| r.strip_suffix('}')) else {
        return parse_key(key).into_iter().collect();
    };
    let words: &[&str] = match vendor {
        "nvidia" => &["nvidia"],
        "amd" => &["amd", "radeon", "advanced micro"],
        "intel" => &["intel"],
        _ => &[],
    };
    reg.list_subkeys(Hive::LocalMachine, DISPLAY_CLASS)
        .unwrap_or_default()
        .into_iter()
        .filter(|sub| sub.len() == 4 && sub.chars().all(|c| c.is_ascii_digit()))
        .map(|sub| format!(r"{DISPLAY_CLASS}\{sub}"))
        .filter(|k| {
            let desc = ["DriverDesc", "ProviderName"]
                .iter()
                .filter_map(|n| reg.read_value(Hive::LocalMachine, k, n).ok().flatten())
                .filter_map(|v| v.as_string().map(str::to_lowercase))
                .collect::<Vec<_>>()
                .join(" ");
            words.iter().any(|w| desc.contains(w))
        })
        .map(|k| (Hive::LocalMachine, k))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct TweakState {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    pub risk: Risk,
    pub recommended: bool,
    pub setting: bool,
    pub restart: Option<String>,
    pub note: Option<String>,
    pub source: Option<String>,
    pub needs_admin: bool,
    /// Conditions met on this PC.
    pub available: bool,
    pub applied: bool,
    /// Sweeplark has an undo record for it.
    pub can_undo: bool,
}

fn service_key(name: &str) -> String {
    format!(r"SYSTEM\CurrentControlSet\Services\{name}")
}

/// Is the tweak currently in effect? Missing services/tasks count as fine.
pub fn is_applied(t: &TweakDef, reg: &dyn RegistryBackend, check_tasks: bool) -> bool {
    for r in &t.registry {
        let keys = resolve_keys(&r.key, reg);
        if keys.is_empty() && r.key.starts_with("{gpu:") {
            continue;
        }
        for (hive, key) in keys {
            let cur = reg.read_value(hive, &key, &r.name).ok().flatten();
            let ok = if r.delete {
                cur.is_none()
            } else if is_flags(r) {
                let want = r.value.as_ref().and_then(|v| v.as_str()).unwrap_or("");
                cur.as_ref().and_then(|c| c.as_string()).is_some_and(|c| flags_contain(c, want))
            } else {
                let want = r.value.as_ref().and_then(|v| to_reg(v, r.kind.as_deref()));
                match (&cur, &want) {
                    (Some(RegValue::Sz(a)), Some(RegValue::Sz(b))) => a.eq_ignore_ascii_case(b),
                    _ => cur == want,
                }
            };
            if !ok {
                return false;
            }
        }
    }
    for s in &t.services {
        if let Ok(Some(RegValue::Dword(cur))) = reg.read_value(Hive::LocalMachine, &service_key(&s.name), "Start")
            && Some(cur) != start_code(&s.start)
        {
            return false;
        }
    }
    if check_tasks {
        for task in &t.tasks {
            if let Some(en) = ops::task_enabled(&task.path)
                && en != task.enabled
            {
                return false;
            }
        }
    }
    match t.action.as_deref() {
        Some("hibernate_off") => ops::hibernate_enabled() != Some(true),
        Some("ultimate_plan") => ops::power_plans().iter().any(|p| p.active && ops::is_ultimate(p)),
        _ => true,
    }
}

pub fn states(set: &TweakSet, reg: &dyn RegistryBackend, ledger: &Ledger, ctx: &TweakContext) -> Vec<TweakState> {
    set.tweaks
        .iter()
        .map(|t| TweakState {
            id: t.id.clone(),
            name: ctx.lang.pick_opt(&t.name, t.name_hi.as_deref()).into(),
            description: ctx.lang.pick_opt(&t.description, t.description_hi.as_deref()).into(),
            category: t.category,
            risk: t.risk,
            recommended: t.recommended,
            setting: t.setting,
            restart: t.restart.clone(),
            note: t.note.as_deref().map(|n| ctx.lang.pick_opt(n, t.note_hi.as_deref()).to_string()),
            source: t.source.clone(),
            needs_admin: t.needs_admin(),
            available: ctx.allows(t),
            applied: is_applied(t, reg, false),
            can_undo: ledger.latest_undoable("tweak", &t.id).ok().flatten().is_some(),
        })
        .collect()
}

fn write_reg(
    reg: &dyn RegistryBackend,
    hive: Hive,
    key: &str,
    name: &str,
    value: Option<&RegValue>,
    steps: &mut Vec<RevertStep>,
) -> Result<()> {
    let previous = reg.read_value(hive, key, name)?;
    if previous.as_ref() == value {
        return Ok(());
    }
    match value {
        Some(v) => reg.write_value(hive, key, name, v)?,
        None => reg.delete_value(hive, key, name)?,
    }
    steps.push(RevertStep::RegistryValue { hive, key: key.into(), name: name.into(), previous });
    Ok(())
}

/// Apply a tweak and record how to undo it. Returns the ledger id.
pub fn apply(t: &TweakDef, reg: &dyn RegistryBackend, ledger: &Ledger, ctx: &TweakContext) -> Result<i64> {
    if !ctx.allows(t) {
        return Err(Error::Other(tr!(ctx.lang, "'{}' is not for this PC.", "'{}' is PC ke liye nahi hai.", t.name)));
    }
    if t.needs_admin() && !ctx.is_admin {
        return Err(Error::NeedsAdmin(t.name.clone()));
    }
    let mut steps = Vec::new();
    let result = (|| -> Result<()> {
        for r in &t.registry {
            for (hive, key) in resolve_keys(&r.key, reg) {
                let value = if r.delete {
                    None
                } else {
                    let cur = reg.read_value(hive, &key, &r.name)?;
                    r.value.as_ref().and_then(|v| target_value(r, v, cur.as_ref()))
                };
                write_reg(reg, hive, &key, &r.name, value.as_ref(), &mut steps)?;
            }
        }
        for s in &t.services {
            let key = service_key(&s.name);
            if reg.read_value(Hive::LocalMachine, &key, "Start")?.is_some() {
                write_reg(reg, Hive::LocalMachine, &key, "Start", Some(&RegValue::Dword(start_code(&s.start).unwrap_or(3))), &mut steps)?;
            }
        }
        for task in &t.tasks {
            if let Some(was) = ops::task_enabled(&task.path)
                && was != task.enabled
            {
                ops::set_task(&task.path, task.enabled)?;
                steps.push(RevertStep::ScheduledTask { path: task.path.clone(), enabled: was });
            }
        }
        match t.action.as_deref() {
            Some("hibernate_off") => {
                if ops::hibernate_enabled() != Some(false) {
                    ops::set_hibernate(false)?;
                    steps.push(RevertStep::Hibernate { enabled: true });
                }
            }
            Some("ultimate_plan") => {
                let before = ops::active_power_plan();
                let guid = ops::add_ultimate_plan()?;
                ops::set_power_plan(&guid)?;
                if let Some(b) = before {
                    steps.push(RevertStep::PowerPlan { guid: b });
                }
            }
            _ => {}
        }
        Ok(())
    })();
    if let Err(e) = result {
        // Roll back what was already changed, then report.
        let _ = crate::revert::apply_all(&steps, reg);
        return Err(e);
    }
    // Already in effect: nothing changed, so nothing to record (an empty record would hide
    // the real one from "switch off").
    if steps.is_empty() {
        return Ok(0);
    }
    let name = &t.name;
    ledger.record(&NewChange {
        category: "tweak".into(),
        title: tr!(ctx.lang, "Optimizer: {name}", "Optimizer: {name}"),
        risk: t.risk,
        status: ChangeStatus::Applied,
        bytes_freed: 0,
        items: steps.len() as u64,
        details: serde_json::json!({ "tweak_id": t.id, "restart": t.restart }),
        revert: steps,
    })
}

/// Switch a tweak off: undo Sweeplark's last change, or for settings write the "off" values.
pub fn unapply(t: &TweakDef, reg: &dyn RegistryBackend, ledger: &Ledger, ctx: &TweakContext) -> Result<()> {
    // Check rights first, so a half-undone tweak (user part back, machine part not) can't happen.
    if t.needs_admin() && !ctx.is_admin {
        return Err(Error::NeedsAdmin(t.name.clone()));
    }
    if let Some(rec) = ledger.latest_undoable("tweak", &t.id)? {
        ledger.undo(rec.id, reg)?;
        return Ok(());
    }
    if !t.registry.iter().all(|r| r.off.is_some()) {
        return Err(Error::Other(
            ctx.lang.pick("This was changed outside Sweeplark; there is nothing to undo.", "Yeh Sweeplark ke bahar badla gaya tha; undo karne ko kuch nahi.").into(),
        ));
    }
    if t.needs_admin() && !ctx.is_admin {
        return Err(Error::NeedsAdmin(t.name.clone()));
    }
    let mut steps = Vec::new();
    for r in &t.registry {
        for (hive, key) in resolve_keys(&r.key, reg) {
            let cur = reg.read_value(hive, &key, &r.name)?;
            let value = r.off.as_ref().and_then(|v| target_value(r, v, cur.as_ref()));
            write_reg(reg, hive, &key, &r.name, value.as_ref(), &mut steps)?;
        }
    }
    let name = &t.name;
    ledger.record(&NewChange {
        category: "tweak.off".into(),
        title: tr!(ctx.lang, "Optimizer off: {name}", "Optimizer band: {name}"),
        risk: t.risk,
        status: ChangeStatus::Applied,
        bytes_freed: 0,
        items: steps.len() as u64,
        details: serde_json::json!({ "tweak_id": t.id }),
        revert: steps,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::registry::MemRegistry;

    fn ctx() -> TweakContext {
        TweakContext { windows_build: 26100, laptop: false, gpu_names: vec!["nvidia geforce rtx 3060".into()], is_admin: true, lang: Lang::En }
    }

    #[test]
    fn builtin_tweaks_are_valid_and_plentiful() {
        let set = TweakSet::builtin();
        assert!(set.tweaks.len() >= 80, "only {} tweaks", set.tweaks.len());
        for t in &set.tweaks {
            assert!(t.id.contains('.'), "{}", t.id);
            if t.setting {
                assert!(t.registry.iter().all(|r| r.off.is_some()), "setting {} needs off values", t.id);
            }
        }
    }

    #[test]
    fn apply_status_and_undo() {
        let mut set = TweakSet::default();
        set.add_file(
            "t.toml",
            r#"
[[tweak]]
id = "privacy.telemetry"
name = "Telemetry off"
description = "x"
category = "privacy"
risk = "safe"
[[tweak.registry]]
key = 'HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection'
name = "AllowTelemetry"
value = 0
[[tweak.registry]]
key = 'HKCU\Control Panel\Desktop'
name = "MenuShowDelay"
value = "0"
[[tweak.service]]
name = "DiagTrack"
start = "disabled"
[[tweak.service]]
name = "NotInstalled"
start = "disabled"
"#,
        )
        .unwrap();
        let reg = MemRegistry::default();
        reg.write_value(Hive::CurrentUser, r"Control Panel\Desktop", "MenuShowDelay", &RegValue::Sz("400".into())).unwrap();
        reg.write_value(Hive::LocalMachine, &service_key("DiagTrack"), "Start", &RegValue::Dword(2)).unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        let t = set.get("privacy.telemetry").unwrap();
        assert!(!is_applied(t, &reg, false));
        apply(t, &reg, &ledger, &ctx()).unwrap();
        assert!(is_applied(t, &reg, false));
        assert_eq!(reg.read_value(Hive::LocalMachine, &service_key("DiagTrack"), "Start").unwrap(), Some(RegValue::Dword(4)));
        assert!(reg.read_value(Hive::LocalMachine, &service_key("NotInstalled"), "Start").unwrap().is_none());
        unapply(t, &reg, &ledger, &ctx()).unwrap();
        assert!(!is_applied(t, &reg, false));
        assert_eq!(reg.read_value(Hive::CurrentUser, r"Control Panel\Desktop", "MenuShowDelay").unwrap(), Some(RegValue::Sz("400".into())));
        assert_eq!(reg.read_value(Hive::LocalMachine, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry").unwrap(), None);
    }

    #[test]
    fn admin_and_conditions() {
        let set = TweakSet::builtin();
        let mut c = ctx();
        c.is_admin = false;
        let hklm = set.tweaks.iter().find(|t| t.needs_admin() && t.action.is_none()).unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        assert!(matches!(apply(hklm, &MemRegistry::default(), &ledger, &c), Err(Error::NeedsAdmin(_))));
        let amd = set.tweaks.iter().find(|t| t.conditions.contains(&"amd".to_string())).unwrap();
        assert!(!c.allows(amd));
    }

    #[test]
    fn gpu_keys_resolve_by_vendor() {
        let reg = MemRegistry::default();
        reg.write_value(Hive::LocalMachine, &format!(r"{DISPLAY_CLASS}\0000"), "DriverDesc", &RegValue::Sz("NVIDIA GeForce RTX 3060".into())).unwrap();
        reg.write_value(Hive::LocalMachine, &format!(r"{DISPLAY_CLASS}\0001"), "DriverDesc", &RegValue::Sz("Intel(R) UHD Graphics".into())).unwrap();
        reg.write_value(Hive::LocalMachine, &format!(r"{DISPLAY_CLASS}\Properties"), "x", &RegValue::Dword(1)).unwrap();
        let keys = resolve_keys("{gpu:nvidia}", &reg);
        assert_eq!(keys.len(), 1);
        assert!(keys[0].1.ends_with("0000"));
    }

    #[test]
    fn flags_merge_keeps_other_entries_and_empty_apply_is_not_recorded() {
        assert_eq!(merge_flags("VRROptimizeEnable=0;", "SwapEffectUpgradeEnable=1;"), "VRROptimizeEnable=0;SwapEffectUpgradeEnable=1;");
        assert_eq!(merge_flags("SwapEffectUpgradeEnable=0;AutoHDREnable=1;", "SwapEffectUpgradeEnable=1;"), "SwapEffectUpgradeEnable=1;AutoHDREnable=1;");
        assert!(flags_contain("A=1;SwapEffectUpgradeEnable=1;", "SwapEffectUpgradeEnable=1;"));
        let mut set = TweakSet::default();
        set.add_file(
            "f.toml",
            r#"
[[tweak]]
id = "gaming.windowed"
name = "Windowed"
description = "x"
category = "gaming"
risk = "safe"
setting = true
[[tweak.registry]]
key = 'HKCU\Software\Microsoft\DirectX\UserGpuPreferences'
name = "DirectXUserGlobalSettings"
kind = "flags"
value = "SwapEffectUpgradeEnable=1;"
off = "SwapEffectUpgradeEnable=0;"
"#,
        )
        .unwrap();
        let reg = MemRegistry::default();
        let key = r"Software\Microsoft\DirectX\UserGpuPreferences";
        reg.write_value(Hive::CurrentUser, key, "DirectXUserGlobalSettings", &RegValue::Sz("VRROptimizeEnable=0;".into())).unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        let t = set.get("gaming.windowed").unwrap();
        let id = apply(t, &reg, &ledger, &ctx()).unwrap();
        assert!(id > 0);
        assert!(is_applied(t, &reg, false));
        assert_eq!(apply(t, &reg, &ledger, &ctx()).unwrap(), 0, "second apply changes nothing and records nothing");
        unapply(t, &reg, &ledger, &ctx()).unwrap();
        assert_eq!(reg.read_value(Hive::CurrentUser, key, "DirectXUserGlobalSettings").unwrap(), Some(RegValue::Sz("VRROptimizeEnable=0;".into())));
    }

    #[test]
    fn settings_switch_off_without_record() {
        let mut set = TweakSet::default();
        set.add_file(
            "s.toml",
            r#"
[[tweak]]
id = "explorer.extensions"
name = "Show file extensions"
description = "x"
category = "explorer"
risk = "safe"
setting = true
[[tweak.registry]]
key = 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced'
name = "HideFileExt"
value = 0
off = 1
"#,
        )
        .unwrap();
        let reg = MemRegistry::default();
        let key = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        reg.write_value(Hive::CurrentUser, key, "HideFileExt", &RegValue::Dword(0)).unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        let t = set.get("explorer.extensions").unwrap();
        assert!(is_applied(t, &reg, false));
        unapply(t, &reg, &ledger, &ctx()).unwrap();
        assert_eq!(reg.read_value(Hive::CurrentUser, key, "HideFileExt").unwrap(), Some(RegValue::Dword(1)));
    }
}
