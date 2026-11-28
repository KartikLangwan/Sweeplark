//! Sweeplark desktop shell: window + tray + background agent around `sweeplark-core`.
//!
//! Lightness: the window (and its WebView, ~100 MB) only exists while it is open. Closing it
//! frees that memory; the tray and the background agent keep running with a few MB.

mod ai_engine;
mod commands;
mod game;
mod host;
mod pc;
mod tray;

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use sweeplark_core::agent::store::AgentStore;
use sweeplark_core::ai::catalog::Catalog;
use sweeplark_core::cleaner::{RuleSet, ScanReport};
use sweeplark_core::ledger::Ledger;
use sweeplark_core::optimize::OptimizePlan;
use sweeplark_core::paths::AppPaths;
use sweeplark_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder};

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
    /// Boost, Game mode and RAM guard have their own monitor: their slow steps (GPU sampling,
    /// memory cleanup) never make the live stats or the tray wait.
    pub boost_monitor: Mutex<sweeplark_core::monitor::Monitor>,
    /// PC facts for tweak conditions, read once.
    pub tweak_ctx: Mutex<Option<sweeplark_core::tweaks::TweakContext>>,
    /// A restore point was made (or tried) in this session.
    pub restore_point_done: AtomicBool,
    /// Page to open when the window is created (tray menu, notifications).
    pub pending_page: Mutex<Option<String>>,
    pub game: Mutex<game::GameState>,
    pub last_uninstalled: Mutex<Option<sweeplark_core::apps::InstalledApp>>,
}

impl AppState {
    fn new() -> Result<Self, String> {
        let paths = AppPaths::discover().map_err(|e| e.to_string())?;
        paths.ensure().map_err(|e| e.to_string())?;
        sweeplark_core::log::init(&paths.data_dir.join("logs"));
        sweeplark_core::log::info(&format!(
            "Sweeplark {} starting ({}{})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            if AppPaths::is_portable() { ", portable" } else { "" }
        ));
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
            boost_monitor: Mutex::new(sweeplark_core::monitor::Monitor::new()),
            tweak_ctx: Mutex::new(None),
            restore_point_done: AtomicBool::new(false),
            pending_page: Mutex::new(None),
            game: Mutex::new(game::GameState::default()),
            last_uninstalled: Mutex::new(None),
        })
    }
}

/// Show the window, creating it (and its WebView) if it was closed.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(p) = page {
            let _ = app.emit("navigate", p);
        }
        return;
    }
    let set_pending = |p: Option<&str>| {
        if let Ok(mut g) = app.state::<AppState>().pending_page.lock() {
            *g = p.map(str::to_string);
        }
    };
    set_pending(page);
    if !sweeplark_core::platform::system::webview2_installed() {
        set_pending(None);
        sweeplark_core::log::error("WebView2 runtime missing");
        sweeplark_core::platform::system::message_box(
            "Sweeplark",
            "Sweeplark needs the Microsoft Edge WebView2 Runtime to show its window.\n\nIt will open the download page now. Install it, then open Sweeplark again.",
        );
        let _ = commands::open_url("https://developer.microsoft.com/microsoft-edge/webview2/".into());
        return;
    }
    let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() else { return };
    match WebviewWindowBuilder::from_config(app, &cfg).and_then(|b| b.build()) {
        Ok(w) => {
            let _ = w.show();
            let _ = w.set_focus();
        }
        Err(e) => {
            sweeplark_core::log::error(&format!("could not create the window: {e}"));
            sweeplark_core::platform::system::message_box("Sweeplark", &format!("Sweeplark could not open its window:\n{e}"));
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

/// Tell an open window that settings changed outside the Settings page.
pub fn settings_changed(app: &AppHandle) {
    let _ = app.emit("settings-changed", ());
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

pub fn quit(app: &AppHandle) {
    sweeplark_core::log::info("quit");
    app.state::<AppState>().ai.unload();
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let background = std::env::args().any(|a| a == "--background");
    // Started by "Restart as administrator": wait (up to 10 s) for the old copy to close, so
    // the single-instance check doesn't hand this start back to it.
    if let Some(pid) = std::env::args().find_map(|a| a.strip_prefix("--wait-parent=").and_then(|p| p.parse::<u32>().ok())) {
        for _ in 0..100 {
            if !sweeplark_core::platform::system::process_alive(pid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    // "--page=clean" (set by "Restart as administrator") opens that page.
    let start_page = std::env::args().find_map(|a| a.strip_prefix("--page=").map(str::to_string));
    std::panic::set_hook(Box::new(|info| {
        sweeplark_core::log::error(&format!("crash: {info}"));
    }));
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app, None)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().args(["--background"]).build())
        .setup(move |app| {
            let state = AppState::new().map_err(Box::<dyn std::error::Error>::from)?;
            if let Some(p) = start_page.clone()
                && let Ok(mut g) = state.pending_page.lock()
            {
                *g = Some(p);
            }
            app.manage(state);
            tray::build(app.handle())?;
            refresh_tray(app.handle());
            commands::apply_autostart(app.handle());
            if !background {
                show_main(app.handle(), None);
            }
            start_agent(app.handle().clone());
            start_ai_janitor(app.handle().clone());
            game::start(app.handle().clone());
            commands::start_update_check(app.handle().clone());
            Ok(())
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
            commands::settings_export,
            commands::settings_import,
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
            commands::take_pending_page,
            commands::logs_open,
            commands::bug_report,
            pc::tweaks_list,
            pc::tweak_set,
            pc::tweaks_apply_recommended,
            pc::live_stats,
            pc::process_list,
            pc::process_end,
            pc::storage_analyze,
            pc::storage_recycle,
            pc::duplicates_find,
            pc::reveal_path,
            pc::tool_run,
            pc::shutdown_schedule,
            pc::windows_tool,
            pc::hardware_info,
            pc::power_plans,
            pc::power_plan_set,
            pc::boost_plan,
            pc::boost_apply,
            pc::boost_restore,
            pc::apps_list,
            pc::app_uninstall,
            pc::app_leftovers,
            pc::app_leftovers_remove,
            pc::app_updates,
            pc::app_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Sweeplark");
    app.run(|_app, event| {
        // Closing the window frees the WebView; Sweeplark keeps running in the tray.
        // Only an explicit Quit (exit code given) ends the app.
        if let RunEvent::ExitRequested { api, code, .. } = event
            && code.is_none()
        {
            api.prevent_exit();
        }
    });
}
