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

/// Tweak conditions: PC facts are read once, admin rights and language every call.
pub(crate) fn ctx(app: &AppHandle) -> TweakContext {
    let state = app.state::<AppState>();
    let lang = state.settings.lock().map(|s| s.language).unwrap_or_default();
    let mut cache = state.tweak_ctx.lock().unwrap_or_else(|e| e.into_inner());
    let mut c = cache.get_or_insert_with(|| TweakContext::system(lang)).clone();
    c.lang = lang;
    c.is_admin = sweeplark_core::platform::system::is_admin();
    c
}

/// Before the first admin-level Optimizer change of this session, make a restore point
/// (Settings can turn this off). Returns a note for the user when it could not be made.
fn restore_point_first(app: &AppHandle, needs_admin: bool) -> Option<String> {
    let state = app.state::<AppState>();
    let want = state.settings.lock().map(|s| s.restore_point_before_tweaks).unwrap_or(true);
    if !want || !needs_admin || !sweeplark_core::platform::system::is_admin() {
        return None;
    }
    if state.restore_point_done.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    match tools::create_restore_point("before Optimizer changes") {
        Ok(()) => Some("restore_point_ok".into()),
        Err(e) => Some(e.to_string()),
    }
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

/// Turn one tweak on or off. Returns a note about the restore point, if one was tried.
#[tauri::command]
pub async fn tweak_set(app: AppHandle, id: String, on: bool) -> CmdResult<Option<String>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let t = state.tweaks.get(&id).ok_or("Unknown tweak")?;
        let c = ctx(&app);
        let note = if on && c.is_admin { restore_point_first(&app, t.needs_admin()) } else { None };
        let ledger = state.ledger.lock().map_err(err)?;
        let reg = system_registry();
        let r = if on {
            tweaks::apply(t, reg.as_ref(), &ledger, &c).map(|_| ()).map_err(err)
        } else {
            tweaks::unapply(t, reg.as_ref(), &ledger, &c).map_err(err)
        };
        crate::commands::logged(&format!("tweak {id}"), r).map(|_| note)
    })
    .await
}

#[derive(Serialize)]
pub struct BulkResult {
    applied: Vec<String>,
    skipped: Vec<(String, String)>,
    restore_point: Option<String>,
}

/// Apply every recommended tweak that is available, not applied and allowed by the trust level.
#[tauri::command]
pub async fn tweaks_apply_recommended(app: AppHandle, ids: Vec<String>) -> CmdResult<BulkResult> {
    blocking(move || {
        let state = app.state::<AppState>();
        let c = ctx(&app);
        let any_admin = ids.iter().filter_map(|id| state.tweaks.get(id)).any(|t| t.needs_admin());
        let restore_point = restore_point_first(&app, any_admin);
        let ledger = state.ledger.lock().map_err(err)?;
        let reg = system_registry();
        let mut out = BulkResult { applied: Vec::new(), skipped: Vec::new(), restore_point };
        let total = ids.len() as u64;
        let mut progress = crate::commands::progress_emitter(&app, "tweaks");
        for (i, id) in ids.iter().enumerate() {
            let Some(t) = state.tweaks.get(id) else { continue };
            progress(i as u64, total, &t.name);
            match tweaks::apply(t, reg.as_ref(), &ledger, &c) {
                Ok(_) => out.applied.push(t.name.clone()),
                Err(e) => {
                    sweeplark_core::log::warn(&format!("tweak {id} skipped: {e}"));
                    out.skipped.push((t.name.clone(), e.to_string()))
                }
            }
        }
        progress(total, total, "");
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn live_stats(app: AppHandle) -> CmdResult<LiveStats> {
    blocking(move || Ok(app.state::<AppState>().monitor.lock().map_err(err)?.sample())).await
}

#[tauri::command]
pub async fn process_list(app: AppHandle) -> CmdResult<Vec<ProcessGroup>> {
    blocking(move || Ok(app.state::<AppState>().monitor.lock().map_err(err)?.processes())).await
}

#[tauri::command]
pub async fn process_end(app: AppHandle, name: String) -> CmdResult<u32> {
    blocking(move || app.state::<AppState>().monitor.lock().map_err(err)?.end_app(&name).map_err(err)).await
}

fn root_or_home(path: Option<String>) -> CmdResult<std::path::PathBuf> {
    let root = path
        .filter(|p| !p.trim().is_empty())
        .map(|p| std::path::PathBuf::from(p.trim()))
        .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
        .ok_or("No folder")?;
    if !root.is_dir() {
        return Err(format!("Folder not found: {}", root.display()));
    }
    Ok(root)
}

#[tauri::command]
pub async fn storage_analyze(app: AppHandle, path: Option<String>) -> CmdResult<StorageReport> {
    blocking(move || {
        let root = root_or_home(path)?;
        let mut progress = crate::commands::progress_emitter(&app, "storage");
        Ok(storage::analyze_with(&root, std::time::Duration::from_secs(90), &mut progress))
    })
    .await
}

#[tauri::command]
pub async fn duplicates_find(app: AppHandle, path: Option<String>, min_mb: Option<u64>) -> CmdResult<storage::DuplicateReport> {
    blocking(move || {
        let root = root_or_home(path)?;
        let mut progress = crate::commands::progress_emitter(&app, "duplicates");
        let min = min_mb.unwrap_or(1).max(1) * 1024 * 1024;
        Ok(storage::find_duplicates_with(&root, min, std::time::Duration::from_secs(120), &mut progress))
    })
    .await
}

#[tauri::command]
pub async fn storage_recycle(path: String) -> CmdResult<()> {
    blocking(move || crate::commands::logged("recycle", storage::recycle(std::path::Path::new(&path)).map_err(err))).await
}

/// Show a file or folder in Explorer.
#[tauri::command]
pub fn reveal_path(path: String) -> CmdResult<()> {
    #[cfg(windows)]
    std::process::Command::new(ops::system_exe("explorer.exe")).arg(format!("/select,{path}")).spawn().map_err(err)?;
    let _ = path;
    Ok(())
}

#[tauri::command]
pub async fn tool_run(tool: Tool) -> CmdResult<String> {
    blocking(move || crate::commands::logged(&format!("tool {tool:?}"), tools::run_tool(tool).map_err(err))).await
}

#[tauri::command]
pub async fn shutdown_schedule(minutes: u32, restart: bool) -> CmdResult<()> {
    blocking(move || tools::schedule_shutdown(minutes, restart).map_err(err)).await
}

#[tauri::command]
pub async fn windows_tool(name: String) -> CmdResult<()> {
    blocking(move || tools::open_windows_tool(&name).map_err(err)).await
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

pub(crate) fn never_list(app: &AppHandle) -> Vec<String> {
    app.state::<AppState>().settings.lock().map(|s| s.boost.never.clone()).unwrap_or_default()
}

#[tauri::command]
pub async fn boost_plan(app: AppHandle) -> CmdResult<sweeplark_core::boost::BoostPlan> {
    blocking(move || {
        let never = never_list(&app);
        let state = app.state::<AppState>();
        let mut m = state.monitor.lock().map_err(err)?;
        Ok(sweeplark_core::boost::plan(&mut m, &never))
    })
    .await
}

#[derive(Serialize)]
pub struct BoostResult {
    #[serde(flatten)]
    report: sweeplark_core::boost::BoostReport,
    /// Ledger entry that undoes it (None = nothing to undo).
    ledger_id: Option<i64>,
}

#[tauri::command]
pub async fn boost_apply(app: AppHandle, options: sweeplark_core::boost::BoostOptions) -> CmdResult<BoostResult> {
    blocking(move || {
        let never = never_list(&app);
        let state = app.state::<AppState>();
        let report = {
            let mut m = state.monitor.lock().map_err(err)?;
            sweeplark_core::boost::apply(&mut m, &options, &never, None)
        };
        let freed = report.ram_before.saturating_sub(report.ram_after);
        let revert = report.revert_steps();
        let ledger_id = state.ledger.lock().ok().and_then(|ledger| {
            ledger
                .record(&sweeplark_core::ledger::NewChange {
                    category: "boost".into(),
                    title: format!("Boost: {} RAM freed, {} apps in Eco mode", sweeplark_core::util::format_bytes(freed), report.eco.len()),
                    risk: sweeplark_core::Risk::Safe,
                    status: sweeplark_core::ledger::ChangeStatus::Applied,
                    bytes_freed: 0,
                    items: u64::from(report.trimmed),
                    details: serde_json::to_value(&report).unwrap_or_default(),
                    revert,
                })
                .ok()
        });
        let undoable = !report.revert_steps().is_empty();
        Ok(BoostResult { report, ledger_id: ledger_id.filter(|_| undoable) })
    })
    .await
}

/// Put back what the last Boost changed (Efficiency mode, power plan).
#[tauri::command]
pub async fn boost_restore(app: AppHandle) -> CmdResult<bool> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ledger = state.ledger.lock().map_err(err)?;
        match ledger.latest_undoable_in("boost").map_err(err)? {
            Some(rec) => ledger.undo(rec.id, system_registry().as_ref()).map(|_| true).map_err(err),
            None => Ok(false),
        }
    })
    .await
}

#[tauri::command]
pub async fn apps_list() -> CmdResult<Vec<sweeplark_core::apps::InstalledApp>> {
    blocking(|| Ok(sweeplark_core::apps::installed(system_registry().as_ref()))).await
}

fn find_app(id: &str) -> CmdResult<sweeplark_core::apps::InstalledApp> {
    sweeplark_core::apps::installed(system_registry().as_ref()).into_iter().find(|a| a.id == id).ok_or_else(|| "App not found".into())
}

#[tauri::command]
pub async fn app_uninstall(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(move || {
        let a = find_app(&id)?;
        // Remember it, so leftovers can be looked up after it disappears from the list.
        if let Ok(mut g) = app.state::<AppState>().last_uninstalled.lock() {
            *g = Some(a.clone());
        }
        crate::commands::logged("uninstall", sweeplark_core::apps::uninstall(&a).map_err(err))
    })
    .await
}

/// Leftover folders of the app last uninstalled (only once it is really gone).
#[tauri::command]
pub async fn app_leftovers(app: AppHandle) -> CmdResult<Vec<String>> {
    blocking(move || {
        let a = app.state::<AppState>().last_uninstalled.lock().map_err(err)?.clone().ok_or("Uninstall an app first")?;
        if sweeplark_core::apps::still_installed(system_registry().as_ref(), &a.id) {
            return Err("The app is still installed. Finish its uninstaller first.".into());
        }
        Ok(sweeplark_core::apps::leftovers(&a))
    })
    .await
}

#[tauri::command]
pub async fn app_updates() -> CmdResult<Vec<sweeplark_core::apps::AppUpdate>> {
    blocking(|| crate::commands::logged("winget", sweeplark_core::apps::updates().map_err(err))).await
}

#[tauri::command]
pub async fn app_update(id: Option<String>) -> CmdResult<()> {
    blocking(move || sweeplark_core::apps::update(id.as_deref()).map_err(err)).await
}
