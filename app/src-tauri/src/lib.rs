//! Sweeplark desktop shell: window + tray + background agent around `sweeplark-core`.

mod ai_engine;
mod commands;
mod host;
mod pc;
mod tray;

use std::sync::Mutex;
use std::time::Duration;

use sweeplark_core::agent::store::AgentStore;
use sweeplark_core::ai::catalog::Catalog;
use sweeplark_core::cleaner::{RuleSet, ScanReport};
use sweeplark_core::ledger::Ledger;
use sweeplark_core::optimize::OptimizePlan;
use sweeplark_core::paths::AppPaths;
use sweeplark_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use crate::ai_engine::AiEngine;

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    pub rules: RuleSet,
    pub catalog: Catalog,
    pub ledger: Mutex<Ledger>,
    pub store: Mutex<AgentStore>,
    pub last_scan: Mutex<Option<ScanReport>>,
    pub last_plan: Mutex<Option<OptimizePlan>>,
    pub ai: AiEngine,
    pub update: Mutex<Option<sweeplark_core::update::UpdateInfo>>,
    pub tweaks: sweeplark_core::tweaks::TweakSet,
    pub monitor: Mutex<sweeplark_core::monitor::Monitor>,
}

impl AppState {
    fn new() -> Result<Self, String> {
        let paths = AppPaths::discover().map_err(|e| e.to_string())?;
        paths.ensure().map_err(|e| e.to_string())?;
        let settings = Settings::load(&paths.settings_file());
        let ledger = Ledger::open(&paths.ledger_db()).map_err(|e| e.to_string())?;
        let store = AgentStore::open(&paths.data_dir.join("agent.db")).map_err(|e| e.to_string())?;
        let catalog = Catalog::builtin();
        Ok(Self {
            ai: AiEngine::new(&paths.data_dir, catalog.clone()),
            paths,
            settings: Mutex::new(settings),
            rules: RuleSet::builtin(),
            catalog,
            ledger: Mutex::new(ledger),
            store: Mutex::new(store),
            last_scan: Mutex::new(None),
            last_plan: Mutex::new(None),
            update: Mutex::new(None),
            tweaks: sweeplark_core::tweaks::TweakSet::builtin(),
            monitor: Mutex::new(sweeplark_core::monitor::Monitor::new()),
        })
    }
}

pub fn show_main(app: &AppHandle, page: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(p) = page {
            let _ = app.emit("navigate", p);
        }
    }
}

/// Tray icon shows an amber dot while something waits for the user.
pub fn refresh_tray(app: &AppHandle) {
    let attention = app
        .state::<AppState>()
        .store
        .lock()
        .map(|s| !s.pending_approvals().unwrap_or_default().is_empty())
        .unwrap_or(false);
    tray::set_attention(app, attention);
}

/// The background agent: one tick a minute on its own thread.
fn start_agent(app: AppHandle) {
    std::thread::Builder::new()
        .name("sweeplark-agent".into())
        .spawn(move || {
            // Let Windows finish logging in before the first sample.
            std::thread::sleep(Duration::from_secs(20));
            loop {
                host::run_tick(&app);
                refresh_tray(&app);
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("agent thread");
}

/// Unload the chat model after it sat idle for the configured time.
fn start_ai_janitor(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(15));
            let state = app.state::<AppState>();
            let secs = state.settings.lock().map(|s| s.ai.unload_after_secs).unwrap_or(120);
            state.ai.unload_if_idle(Duration::from_secs(secs.max(30)));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let background = std::env::args().any(|a| a == "--background");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app, None)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().args(["--background"]).build())
        .setup(move |app| {
            let state = AppState::new().map_err(Box::<dyn std::error::Error>::from)?;
            app.manage(state);
            tray::build(app.handle())?;
            tray::start_stats(app.handle().clone());
            refresh_tray(app.handle());
            commands::apply_autostart(app.handle());
            if !background {
                show_main(app.handle(), None);
            }
            start_agent(app.handle().clone());
            start_ai_janitor(app.handle().clone());
            commands::start_update_check(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Sweeplark running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_health,
            commands::scan,
            commands::clean,
            commands::startup_list,
            commands::startup_set,
            commands::history,
            commands::undo,
            commands::optimize_plan,
            commands::optimize_apply,
            commands::settings_get,
            commands::settings_set,
            commands::inbox_list,
            commands::inbox_read,
            commands::approvals_list,
            commands::approval_decide,
            commands::ai_status,
            commands::ai_install,
            commands::ai_remove,
            commands::ai_chat,
            commands::ai_unload,
            commands::efficiency_mode,
            commands::open_windows_security,
            commands::relaunch_as_admin,
            commands::update_check,
            commands::update_install,
            commands::update_skip,
            commands::open_url,
            pc::tweaks_list,
            pc::tweak_set,
            pc::tweaks_apply_recommended,
            pc::live_stats,
            pc::process_list,
            pc::process_end,
            pc::storage_analyze,
            pc::storage_recycle,
            pc::reveal_path,
            pc::tool_run,
            pc::shutdown_schedule,
            pc::windows_tool,
            pc::hardware_info,
            pc::power_plans,
            pc::power_plan_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sweeplark");
}
