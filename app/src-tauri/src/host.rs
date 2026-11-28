//! Connects the core background agent to the real app: notifications, the AI engine,
//! and the PC itself.

use sweeplark_core::agent::store::AgentStore;
use sweeplark_core::agent::{Agent, AgentHost};
use sweeplark_core::ai::review::{self, ReviewItem, Verdict};
use sweeplark_core::ai::{self};
use sweeplark_core::ledger::Ledger;
use sweeplark_core::platform::registry::system_registry;
use sweeplark_core::platform::system;
use sweeplark_core::settings::Settings;
use sweeplark_core::startup::{self, KnownApps, StartupFolders, StartupItem};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

struct TauriHost<'a> {
    app: &'a AppHandle,
    settings: Settings,
    /// The model was not loaded before this tick, so the tick must unload it.
    loaded_by_tick: bool,
}

impl TauriHost<'_> {
    fn ai<T>(&mut self, f: impl FnOnce(&ai::client::LlamaClient, &ai::catalog::ModelSpec) -> T) -> Option<T> {
        let state = self.app.state::<AppState>();
        let was_loaded = state.ai.is_loaded();
        let out = state.ai.with_model(&self.settings, f).ok();
        if !was_loaded && state.ai.is_loaded() {
            self.loaded_by_tick = true;
        }
        out
    }
}

impl AgentHost for TauriHost<'_> {
    fn notify(&mut self, title: &str, body: &str, sound: bool) {
        let mut b = self.app.notification().builder().title(title).body(body);
        if sound {
            b = b.sound("Default");
        }
        let _ = b.show();
        let _ = self.app.emit("inbox-changed", ());
    }

    fn narrate(&mut self, kind: &str, points: &[String], facts: &str) -> Option<String> {
        let lang = self.settings.language;
        self.ai(|client, _| ai::narrate(client, kind, points, facts, lang)).flatten()
    }

    fn review(&mut self, items: &[ReviewItem]) -> Option<Vec<Verdict>> {
        self.ai(|client, _| review::review(client, items))
    }

    fn release_model(&mut self) {
        if self.loaded_by_tick {
            self.app.state::<AppState>().ai.unload_gentle();
            self.loaded_by_tick = false;
        }
    }

    fn idle_seconds(&self) -> u64 {
        system::idle_seconds()
    }

    fn startup_items(&mut self) -> Vec<StartupItem> {
        startup::list(system_registry().as_ref(), &StartupFolders::system(), &KnownApps::builtin().with_lang(self.settings.language))
            .unwrap_or_default()
    }
}

pub fn run_tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = match state.settings.lock() {
        Ok(s) => s.clone(),
        Err(_) => return,
    };
    // Own connections: the UI keeps working while a tick runs (SQLite WAL).
    let (Ok(store), Ok(ledger)) = (
        AgentStore::open(&state.paths.data_dir.join("agent.db")),
        Ledger::open(&state.paths.ledger_db()),
    ) else {
        return;
    };
    let agent = Agent { store: &store, ledger: &ledger, rules: &state.rules };
    let mut host = TauriHost { app, settings: settings.clone(), loaded_by_tick: false };
    if let Ok(report) = agent.tick(&settings, chrono::Utc::now(), &mut host)
        && (report.daily || report.weekly || report.auto_clean || !report.alerts.is_empty())
    {
        let _ = app.emit("inbox-changed", ());
    }
}
