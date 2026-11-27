//! PC manager commands: optimizer tweaks, live monitor, processes, storage, tools.

use serde::Serialize;
use sweeplark_core::monitor::{LiveStats, ProcessGroup};
use sweeplark_core::platform::ops::{self, PowerPlan};
use sweeplark_core::platform::registry::system_registry;
use sweeplark_core::storage::{self, StorageReport};
use sweeplark_core::tools::{self, HardwareInfo, Tool};
use sweeplark_core::tweaks::{self, TweakContext, TweakState};
use tauri::{AppHandle, Manager};

use crate::AppState;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

fn ctx(app: &AppHandle) -> TweakContext {
    let lang = app.state::<AppState>().settings.lock().map(|s| s.language).unwrap_or_default();
    TweakContext::system(lang)
}

#[tauri::command]
pub async fn tweaks_list(app: AppHandle) -> CmdResult<Vec<TweakState>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ledger = state.ledger.lock().map_err(err)?;
        Ok(tweaks::states(&state.tweaks, system_registry().as_ref(), &ledger, &ctx(&app)))
    })
    .await
}

/// Turn one tweak on or off.
#[tauri::command]
pub async fn tweak_set(app: AppHandle, id: String, on: bool) -> CmdResult<()> {
    blocking(move || {
        let state = app.state::<AppState>();
        let t = state.tweaks.get(&id).ok_or("Unknown tweak")?;
        let ledger = state.ledger.lock().map_err(err)?;
        let reg = system_registry();
        if on {
            tweaks::apply(t, reg.as_ref(), &ledger, &ctx(&app)).map(|_| ()).map_err(err)
        } else {
            tweaks::unapply(t, reg.as_ref(), &ledger, &ctx(&app)).map_err(err)
        }
    })
    .await
}

#[derive(Serialize)]
pub struct BulkResult {
    applied: Vec<String>,
    skipped: Vec<(String, String)>,
}

/// Apply every recommended tweak that is available, not applied and allowed by the trust level.
#[tauri::command]
pub async fn tweaks_apply_recommended(app: AppHandle, ids: Vec<String>) -> CmdResult<BulkResult> {
    blocking(move || {
        let state = app.state::<AppState>();
        let c = ctx(&app);
        let ledger = state.ledger.lock().map_err(err)?;
        let reg = system_registry();
        let mut out = BulkResult { applied: Vec::new(), skipped: Vec::new() };
        for id in ids {
            let Some(t) = state.tweaks.get(&id) else { continue };
            match tweaks::apply(t, reg.as_ref(), &ledger, &c) {
                Ok(_) => out.applied.push(t.name.clone()),
                Err(e) => out.skipped.push((t.name.clone(), e.to_string())),
            }
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub fn live_stats(app: AppHandle) -> CmdResult<LiveStats> {
    Ok(app.state::<AppState>().monitor.lock().map_err(err)?.sample())
}

#[tauri::command]
pub async fn process_list(app: AppHandle) -> CmdResult<Vec<ProcessGroup>> {
    blocking(move || Ok(app.state::<AppState>().monitor.lock().map_err(err)?.processes())).await
}

#[tauri::command]
pub async fn process_end(app: AppHandle, name: String) -> CmdResult<u32> {
    blocking(move || app.state::<AppState>().monitor.lock().map_err(err)?.end_app(&name).map_err(err)).await
}

#[tauri::command]
pub async fn storage_analyze(path: Option<String>) -> CmdResult<StorageReport> {
    blocking(move || {
        let root = path
            .map(std::path::PathBuf::from)
            .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
            .ok_or("No folder")?;
        Ok(storage::analyze(&root, std::time::Duration::from_secs(25)))
    })
    .await
}

#[tauri::command]
pub async fn storage_recycle(path: String) -> CmdResult<()> {
    blocking(move || storage::recycle(std::path::Path::new(&path)).map_err(err)).await
}

/// Show a file or folder in Explorer.
#[tauri::command]
pub fn reveal_path(path: String) -> CmdResult<()> {
    #[cfg(windows)]
    std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn().map_err(err)?;
    let _ = path;
    Ok(())
}

#[tauri::command]
pub async fn tool_run(tool: Tool) -> CmdResult<String> {
    blocking(move || tools::run_tool(tool).map_err(err)).await
}

#[tauri::command]
pub fn shutdown_schedule(minutes: u32, restart: bool) -> CmdResult<()> {
    tools::schedule_shutdown(minutes, restart).map_err(err)
}

#[tauri::command]
pub fn windows_tool(name: String) -> CmdResult<()> {
    tools::open_windows_tool(&name).map_err(err)
}

#[tauri::command]
pub async fn hardware_info() -> CmdResult<HardwareInfo> {
    blocking(|| Ok(tools::hardware_info())).await
}

#[tauri::command]
pub async fn power_plans() -> CmdResult<Vec<PowerPlan>> {
    blocking(|| Ok(ops::power_plans())).await
}

#[tauri::command]
pub async fn power_plan_set(guid: Option<String>) -> CmdResult<()> {
    blocking(move || match guid {
        Some(g) => ops::set_power_plan(&g).map_err(err),
        None => ops::add_ultimate_plan().and_then(|g| ops::set_power_plan(&g)).map_err(err),
    })
    .await
}

#[tauri::command]
pub async fn boost_plan(app: AppHandle) -> CmdResult<sweeplark_core::boost::BoostPlan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let mut m = state.monitor.lock().map_err(err)?;
        Ok(sweeplark_core::boost::plan(&mut m))
    })
    .await
}

#[tauri::command]
pub async fn boost_apply(app: AppHandle, options: sweeplark_core::boost::BoostOptions) -> CmdResult<sweeplark_core::boost::BoostReport> {
    blocking(move || {
        let state = app.state::<AppState>();
        let report = {
            let mut m = state.monitor.lock().map_err(err)?;
            sweeplark_core::boost::apply(&mut m, &options)
        };
        let freed = report.ram_before.saturating_sub(report.ram_after);
        if let Ok(ledger) = state.ledger.lock() {
            let _ = ledger.record(&sweeplark_core::ledger::NewChange {
                category: "optimize".into(),
                title: format!("Boost: {} RAM freed, {} apps in Eco mode", sweeplark_core::util::format_bytes(freed), report.eco.len()),
                risk: sweeplark_core::Risk::Safe,
                status: sweeplark_core::ledger::ChangeStatus::Applied,
                bytes_freed: 0,
                items: u64::from(report.trimmed),
                details: serde_json::to_value(&report).unwrap_or_default(),
                revert: Vec::new(),
            });
        }
        Ok(report)
    })
    .await
}
