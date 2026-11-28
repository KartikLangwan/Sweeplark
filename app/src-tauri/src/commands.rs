//! Commands the UI calls. Heavy work runs on a blocking thread so the window stays smooth.

use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use sweeplark_core::agent::autoclean::{self, ApprovalDetail};
use sweeplark_core::agent::store::{Approval, ApprovalStatus, Message};
use sweeplark_core::ai::catalog::{self, ModelSpec, Tier};
use sweeplark_core::ai::client::ChatMessage;
use sweeplark_core::ai::facts::{Facts, PcSource};
use sweeplark_core::ai::runtime::{self, AiInstallState};
use sweeplark_core::ai::{self, AssistantReply};
use sweeplark_core::branding;
use sweeplark_core::cleaner::{self, CleanReport, ScanContext, ScanOptions, ScanReport};
use sweeplark_core::health::{self, HealthSnapshot};
use sweeplark_core::ledger::{ChangeRecord, ChangeStatus, Ledger, LedgerSummary, NewChange};
use sweeplark_core::optimize::{self, OptimizePlan, OptimizeReport};
use sweeplark_core::platform::processes::ProcessList;
use sweeplark_core::platform::registry::system_registry;
use sweeplark_core::platform::system;
use sweeplark_core::settings::Settings;
use sweeplark_core::startup::{self, KnownApps, StartupFolders, StartupItem};
use sweeplark_core::{Risk, TrustLevel};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::AppState;
use crate::ai_engine::InstallProgress;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

/// Progress of a long task for the window's progress bar: `job-progress` events.
#[derive(Clone, Serialize)]
pub struct JobProgress {
    pub job: String,
    pub done: u64,
    pub total: u64,
    pub text: String,
}

pub fn progress_emitter(app: &AppHandle, job: &str) -> impl FnMut(u64, u64, &str) + use<> {
    let app = app.clone();
    let job = job.to_string();
    let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
    move |done, total, text| {
        if last.elapsed() >= std::time::Duration::from_millis(150) || (total > 0 && done >= total) {
            last = std::time::Instant::now();
            let _ = app.emit("job-progress", JobProgress { job: job.clone(), done, total, text: text.to_string() });
        }
    }
}

/// Log an error from a command and pass it on to the window.
pub fn logged<T>(what: &str, r: CmdResult<T>) -> CmdResult<T> {
    if let Err(e) = &r {
        sweeplark_core::log::warn(&format!("{what}: {e}"));
    }
    r
}

/// Change settings from the app itself (tray, AI download ...): saves, bumps the revision
/// and tells an open window.
pub fn update_settings(app: &AppHandle, f: impl FnOnce(&mut Settings)) {
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.settings.lock() {
        f(&mut s);
        s.rev += 1;
        if let Err(e) = s.save(&state.paths.settings_file()) {
            sweeplark_core::log::warn(&format!("could not save settings: {e}"));
        }
    }
    crate::settings_changed(app);
}

/// A long job gets its own database connection (WAL), so History, undo and the UI never
/// wait behind it.
fn own_ledger(app: &AppHandle) -> CmdResult<Ledger> {
    Ledger::open(&app.state::<AppState>().paths.ledger_db()).map_err(err)
}

fn settings_of(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().map(|s| s.clone()).unwrap_or_default()
}

#[derive(Serialize)]
pub struct AppInfo {
    developer: &'static str,
    developer_title: &'static str,
    developer_tagline: &'static str,
    developer_url: &'static str,
    repo: &'static str,
    portable: bool,
    name: &'static str,
    tagline: &'static str,
    tagline_hi: &'static str,
    version: &'static str,
    is_admin: bool,
    os: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        developer: branding::DEVELOPER_NAME,
        developer_title: branding::DEVELOPER_TITLE,
        developer_tagline: branding::DEVELOPER_TAGLINE,
        developer_url: branding::DEVELOPER_URL,
        repo: branding::UPDATE_REPO,
        portable: sweeplark_core::paths::AppPaths::is_portable(),
        name: branding::APP_NAME,
        tagline: branding::TAGLINE,
        tagline_hi: branding::TAGLINE_HI,
        version: env!("CARGO_PKG_VERSION"),
        is_admin: system::is_admin(),
        os: std::env::consts::OS.into(),
    }
}

#[tauri::command]
pub async fn get_health(slow: bool) -> CmdResult<HealthSnapshot> {
    blocking(move || Ok(health::snapshot(slow))).await
}

#[tauri::command]
pub async fn scan(app: AppHandle, expert: bool, privacy: bool) -> CmdResult<ScanReport> {
    blocking(move || {
        let s = settings_of(&app);
        let ctx = ScanContext::system(&s);
        let mut opts = ScanOptions::from_settings(&s);
        if expert && s.trust_level == TrustLevel::Expert {
            opts.max_risk = Risk::Expert;
        }
        opts.include_privacy = privacy && s.privacy_consent;
        let mut progress = progress_emitter(&app, "scan");
        let report = cleaner::scan_with(&app.state::<AppState>().rules, &ctx, &opts, &mut progress);
        *app.state::<AppState>().last_scan.lock().map_err(err)? = Some(report.clone());
        Ok(report)
    })
    .await
}

#[tauri::command]
pub async fn clean(app: AppHandle, ids: Vec<String>, confirmed: bool, approved_files: Vec<String>) -> CmdResult<CleanReport> {
    blocking(move || {
        let state = app.state::<AppState>();
        let s = settings_of(&app);
        let report = state.last_scan.lock().map_err(err)?.clone().ok_or("Run a scan first")?;
        let guard = sweeplark_core::safety::SafetyGuard::for_current_user(s.whitelist.clone());
        let approved: Vec<std::path::PathBuf> = approved_files.into_iter().map(Into::into).collect();
        let ledger = own_ledger(&app)?;
        let res = logged("clean", cleaner::clean(&report, &ids, confirmed, &approved, &guard, &ProcessList::current(), &state.rules, Some(&ledger))
            .map_err(err))?;
        *state.last_scan.lock().map_err(err)? = None;
        Ok(res)
    })
    .await
}

#[tauri::command]
pub async fn startup_list(app: AppHandle) -> CmdResult<Vec<StartupItem>> {
    blocking(move || {
        let lang = settings_of(&app).language;
        let known = KnownApps::builtin().with_lang(lang);
        let mut items = startup::list(system_registry().as_ref(), &StartupFolders::system(), &known).map_err(err)?;
        items.extend(startup::logon_tasks(&known));
        items.sort_by_key(|i| i.name.to_lowercase());
        Ok(items)
    })
    .await
}

#[tauri::command]
pub async fn startup_set(app: AppHandle, id: String, enabled: bool) -> CmdResult<i64> {
    blocking(move || {
        let reg = system_registry();
        let known = KnownApps::builtin();
        let mut items = startup::list(reg.as_ref(), &StartupFolders::system(), &known).map_err(err)?;
        if id.starts_with("task:") {
            items.extend(startup::logon_tasks(&known));
        }
        let item = items.iter().find(|i| i.id == id).ok_or("Startup item not found")?;
        let state = app.state::<AppState>();
        let ledger = state.ledger.lock().map_err(err)?;
        logged("startup", startup::set_enabled_recorded(reg.as_ref(), &ledger, item, enabled, system::is_admin(), settings_of(&app).language).map_err(err))
    })
    .await
}

#[derive(Serialize)]
pub struct History {
    summary: LedgerSummary,
    rows: Vec<ChangeRecord>,
}

#[tauri::command]
pub async fn history(app: AppHandle, limit: usize) -> CmdResult<History> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ledger = state.ledger.lock().map_err(err)?;
        Ok(History { summary: ledger.summary().map_err(err)?, rows: ledger.list(limit).map_err(err)? })
    })
    .await
}

#[tauri::command]
pub async fn undo(app: AppHandle, id: i64) -> CmdResult<ChangeRecord> {
    blocking(move || logged("undo", app.state::<AppState>().ledger.lock().map_err(err)?.undo(id, system_registry().as_ref()).map_err(err)))
        .await
}

#[tauri::command]
pub async fn optimize_plan(app: AppHandle) -> CmdResult<OptimizePlan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let plan = optimize::plan(&state.rules, &settings_of(&app), system_registry().as_ref()).map_err(err)?;
        *state.last_plan.lock().map_err(err)? = Some(plan.clone());
        Ok(plan)
    })
    .await
}

#[tauri::command]
pub async fn optimize_apply(app: AppHandle) -> CmdResult<OptimizeReport> {
    blocking(move || {
        let state = app.state::<AppState>();
        // Kept until it worked, so a failed attempt can simply be tried again.
        let plan = state.last_plan.lock().map_err(err)?.clone().ok_or("Run a scan first")?;
        let ledger = own_ledger(&app)?;
        let report = optimize::apply(&plan, &state.rules, &settings_of(&app), &ledger).map_err(err)?;
        *state.last_plan.lock().map_err(err)? = None;
        Ok(report)
    })
    .await
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.lock().map_err(err)?.clone())
}

pub fn apply_autostart(app: &AppHandle) {
    let want = settings_of(app).agent.start_with_windows;
    let launcher = app.autolaunch();
    if want {
        // Always (re)write it: after an update or moving the exe the old path would be stale.
        let _ = launcher.enable();
    } else if launcher.is_enabled().unwrap_or(false) {
        let _ = launcher.disable();
    }
    crate::tray::set_autostart_check(want);
}

#[tauri::command]
pub async fn settings_set(app: AppHandle, settings: Settings) -> CmdResult<Settings> {
    blocking(move || {
        let state = app.state::<AppState>();
        let mut settings = settings.sanitized();
        {
            let mut cur = state.settings.lock().map_err(err)?;
            if settings.rev != cur.rev {
                // Changed meanwhile (tray, AI download): the window reloads instead of overwriting.
                return Err("settings_changed".into());
            }
            settings.rev += 1;
            settings.save(&state.paths.settings_file()).map_err(err)?;
            *cur = settings.clone();
        }
        apply_autostart(&app);
        Ok(settings)
    })
    .await
}

/// Settings as pretty JSON, to copy to another PC.
#[tauri::command]
pub fn settings_export(state: State<'_, AppState>) -> CmdResult<String> {
    serde_json::to_string_pretty(&*state.settings.lock().map_err(err)?).map_err(err)
}

/// Load settings from JSON pasted by the user (checked before it is used).
#[tauri::command]
pub async fn settings_import(app: AppHandle, json: String) -> CmdResult<Settings> {
    blocking(move || {
        let parsed: Settings = serde_json::from_str(json.trim()).map_err(|e| format!("Not valid Sweeplark settings: {e}"))?;
        let mut parsed = parsed.sanitized();
        let state = app.state::<AppState>();
        {
            let mut cur = state.settings.lock().map_err(err)?;
            parsed.rev = cur.rev + 1;
            parsed.save(&state.paths.settings_file()).map_err(err)?;
            *cur = parsed.clone();
        }
        apply_autostart(&app);
        sweeplark_core::log::info("settings imported");
        Ok(parsed)
    })
    .await
}

#[derive(Serialize)]
pub struct Inbox {
    messages: Vec<Message>,
    unread: u64,
    pending_approvals: usize,
}

#[tauri::command]
pub fn inbox_list(state: State<'_, AppState>) -> CmdResult<Inbox> {
    let store = state.store.lock().map_err(err)?;
    Ok(Inbox {
        messages: store.messages(100).map_err(err)?,
        unread: store.unread_count().map_err(err)?,
        pending_approvals: store.pending_approvals().map_err(err)?.len(),
    })
}

#[tauri::command]
pub fn inbox_read(state: State<'_, AppState>, id: Option<i64>) -> CmdResult<()> {
    state.store.lock().map_err(err)?.mark_read(id).map_err(err)
}

#[tauri::command]
pub fn approvals_list(state: State<'_, AppState>) -> CmdResult<Vec<Approval>> {
    state.store.lock().map_err(err)?.pending_approvals().map_err(err)
}

#[tauri::command]
pub async fn approval_decide(app: AppHandle, id: i64, approve: bool, files: Vec<String>) -> CmdResult<Option<CleanReport>> {
    let res = blocking({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            // Check and mark in one step (under the lock), so a double click can't run it twice.
            let approval = {
                let store = state.store.lock().map_err(err)?;
                let a = store.approval(id).map_err(err)?.ok_or("Request not found")?;
                if a.status != ApprovalStatus::Pending {
                    return Err("This request was already decided".into());
                }
                store
                    .set_approval_status(id, if approve { ApprovalStatus::Approved } else { ApprovalStatus::Declined })
                    .map_err(err)?;
                a
            };
            if !approve {
                return Ok(None);
            }
            let detail: ApprovalDetail = serde_json::from_value(approval.detail).map_err(err)?;
            let s = settings_of(&app);
            let ctx = ScanContext::system(&s);
            let ledger = own_ledger(&app)?;
            match autoclean::apply_approval(&detail, &files, &state.rules, &s, &ctx, &ledger) {
                Ok(report) => Ok(Some(report)),
                Err(e) => {
                    // Nothing was done: let the user try again.
                    let _ = state.store.lock().map(|st| st.set_approval_status(id, ApprovalStatus::Pending));
                    Err(e.to_string())
                }
            }
        }
    })
    .await;
    crate::refresh_tray(&app);
    let _ = app.emit("inbox-changed", ());
    res
}

#[derive(Serialize)]
pub struct ModelInfo {
    #[serde(flatten)]
    spec: ModelSpec,
    installed: bool,
    fits: bool,
}

#[derive(Serialize)]
pub struct AiStatus {
    enabled: bool,
    tier: Option<Tier>,
    recommended: Tier,
    models: Vec<ModelInfo>,
    install: AiInstallState,
    loaded: Option<Tier>,
    progress: Option<InstallProgress>,
    ram_total: u64,
    has_gpu: bool,
}

#[tauri::command]
pub async fn ai_status(app: AppHandle) -> CmdResult<AiStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        let s = settings_of(&app);
        let h = health::snapshot(false);
        let install = state.ai.install_state();
        let models = state
            .catalog
            .models
            .iter()
            .map(|m| ModelInfo {
                installed: install.installed_tiers.contains(&m.tier),
                fits: m.ram_needed() < h.memory_total / 2,
                spec: m.clone(),
            })
            .collect();
        Ok(AiStatus {
            enabled: s.ai.enabled,
            tier: s.ai.tier,
            recommended: catalog::recommend(h.memory_total, h.has_dedicated_gpu()),
            models,
            install,
            loaded: state.ai.loaded_tier(),
            progress: state.ai.progress.lock().map_err(err)?.clone(),
            ram_total: h.memory_total,
            has_gpu: h.has_dedicated_gpu(),
        })
    })
    .await
}

/// Download the engine (if needed) and a model, then switch AI on. Progress arrives as
/// `ai-progress` events. Call again with `cancel = true` to stop.
#[tauri::command]
pub fn ai_install(app: AppHandle, tier: Tier, cancel: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if cancel {
        state.ai.cancel.store(true, Ordering::Relaxed);
        return Ok(());
    }
    // Marked busy before the thread starts, so a double click can't start two downloads.
    if state.ai.installing.swap(true, Ordering::SeqCst) {
        return Err("A download is already running".into());
    }
    state.ai.cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        struct Done<'a>(&'a std::sync::atomic::AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _done = Done(&state.ai.installing);
        let emit = |p: InstallProgress| {
            state.ai.set_progress(p.clone());
            let _ = app.emit("ai-progress", p);
        };
        let s = settings_of(&app);
        let result = (|| -> Result<(), String> {
            if !state.ai.install_state().runtime_installed {
                let gpu = s.ai.use_gpu && health::snapshot(false).has_dedicated_gpu();
                let mut last = 0u64;
                runtime::install_runtime(&state.ai.paths, &state.ai.catalog, gpu, &mut |done, total| {
                    if done - last > 512 * 1024 || done == total {
                        last = done;
                        emit(InstallProgress { stage: "runtime".into(), tier: Some(tier), done, total, error: None });
                    }
                    !state.ai.cancelled()
                })
                .map_err(err)?;
            }
            let spec = state.ai.catalog.get(tier).clone();
            let mut last = 0u64;
            runtime::install_model(&state.ai.paths, &spec, &mut |done, total| {
                if done - last > 2 * 1024 * 1024 || done == total {
                    last = done;
                    emit(InstallProgress { stage: "model".into(), tier: Some(tier), done, total, error: None });
                }
                !state.ai.cancelled()
            })
            .map_err(err)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                update_settings(&app, |st| {
                    st.ai.enabled = true;
                    st.ai.tier = Some(tier);
                });
                sweeplark_core::log::info(&format!("AI model installed: {tier:?}"));
                emit(InstallProgress { stage: "done".into(), tier: Some(tier), done: 1, total: 1, error: None });
            }
            Err(e) => {
                sweeplark_core::log::warn(&format!("AI install failed: {e}"));
                emit(InstallProgress { stage: "error".into(), tier: Some(tier), done: 0, total: 0, error: Some(e) })
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn ai_remove(app: AppHandle, tier: Tier) -> CmdResult<()> {
    blocking(move || {
        let state = app.state::<AppState>();
        if state.ai.loaded_tier() == Some(tier) {
            state.ai.unload();
        }
        runtime::remove_model(&state.ai.paths, state.catalog.get(tier)).map_err(err)?;
        if state.settings.lock().map(|s| s.ai.tier == Some(tier)).unwrap_or(false) {
            update_settings(&app, |s| {
                s.ai.enabled = false;
                s.ai.tier = None;
            });
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn ai_unload(app: AppHandle) -> CmdResult<()> {
    blocking(move || {
        app.state::<AppState>().ai.unload();
        Ok(())
    })
    .await
}

/// Real PC data for the assistant's tools.
struct RealPc {
    app: AppHandle,
}

impl PcSource for RealPc {
    fn health(&self) -> HealthSnapshot {
        health::snapshot(true)
    }
    fn scan(&self) -> Option<ScanReport> {
        let s = settings_of(&self.app);
        let ctx = ScanContext::system(&s);
        Some(cleaner::scan(&self.app.state::<AppState>().rules, &ctx, &ScanOptions::from_settings(&s)))
    }
    fn startup(&self) -> Vec<StartupItem> {
        let lang = settings_of(&self.app).language;
        startup::list(system_registry().as_ref(), &StartupFolders::system(), &KnownApps::builtin().with_lang(lang)).unwrap_or_default()
    }
    fn history(&self) -> Option<(LedgerSummary, Vec<ChangeRecord>)> {
        let state = self.app.state::<AppState>();
        let l = state.ledger.lock().ok()?;
        Some((l.summary().ok()?, l.list(5).ok()?))
    }
    fn tweaks(&self) -> Vec<sweeplark_core::tweaks::TweakState> {
        let state = self.app.state::<AppState>();
        let c = crate::pc::ctx(&self.app);
        let Ok(ledger) = state.ledger.lock() else { return Vec::new() };
        sweeplark_core::tweaks::states(&state.tweaks, system_registry().as_ref(), &ledger, &c)
    }
    fn live(&self) -> Option<sweeplark_core::monitor::LiveStats> {
        Some(self.app.state::<AppState>().monitor.lock().ok()?.sample())
    }
    fn background(&self) -> Option<sweeplark_core::boost::BoostPlan> {
        let never = crate::pc::never_list(&self.app);
        let state = self.app.state::<AppState>();
        let mut m = state.boost_monitor.lock().ok()?;
        Some(sweeplark_core::boost::plan(&mut m, &never))
    }
    fn apps(&self) -> Vec<sweeplark_core::apps::InstalledApp> {
        sweeplark_core::apps::installed(system_registry().as_ref())
    }
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum ChatEvent {
    Status(String),
    Token(String),
}

#[derive(Deserialize)]
pub struct ChatTurn {
    role: String,
    content: String,
}

#[tauri::command]
pub async fn ai_chat(app: AppHandle, question: String, history: Vec<ChatTurn>, on_event: Channel<ChatEvent>) -> CmdResult<AssistantReply> {
    blocking(move || {
        let state = app.state::<AppState>();
        let s = settings_of(&app);
        let pc = RealPc { app: app.clone() };
        let facts = Facts::new(&pc, s.language);
        let history: Vec<ChatMessage> = history
            .into_iter()
            .filter(|t| t.role == "user" || t.role == "assistant")
            .map(|t| ChatMessage { role: t.role, content: t.content })
            .collect();
        let _ = on_event.send(ChatEvent::Status(s.language.pick("Checking your PC…", "PC ki jaankari dekh raha hoon…").into()));
        let mut stream = |piece: &str| on_event.send(ChatEvent::Token(piece.to_string())).is_ok();
        if state.ai.usable_spec(&s).is_some() {
            if !state.ai.is_loaded() {
                let _ = on_event.send(ChatEvent::Status(s.language.pick("Loading the AI model…", "AI model load ho raha hai…").into()));
            }
            if let Ok(reply) = state.ai.with_model(&s, |client, spec| ai::answer(&question, &history, &facts, Some((client, spec)), &mut stream)) {
                return Ok(reply);
            }
        }
        Ok(ai::answer(&question, &history, &facts, None, &mut stream))
    })
    .await
}

/// Windows "Efficiency mode" for every process of an app (e.g. all chrome.exe).
#[tauri::command]
pub async fn efficiency_mode(app: AppHandle, name: String, on: bool) -> CmdResult<u32> {
    blocking(move || {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
        if health::is_system_process(&name) {
            return Err("Windows system processes cannot be changed".into());
        }
        let mut sys = System::new_with_specifics(RefreshKind::nothing()).map_err(err)?;
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let want = name.to_lowercase();
        let own = std::process::id();
        let mut changed = 0;
        let mut revert = Vec::new();
        for (pid, p) in sys.processes() {
            let pname = p.name().to_string_lossy().to_lowercase();
            if pid.as_u32() == own || p.parent().is_some_and(|pp| pp.as_u32() == own) {
                continue; // never our own window or AI engine
            }
            if pname == want || pname.trim_end_matches(".exe") == want {
                let before = system::priority_class(pid.as_u32());
                if system::set_efficiency_mode(pid.as_u32(), on).is_ok() {
                    changed += 1;
                    if on && let Some(prio) = before.filter(|p| *p != 0x40) {
                        revert.push(sweeplark_core::revert::RevertStep::ProcessPriority { pid: pid.as_u32(), name: pname.clone(), priority: prio });
                    }
                }
            }
        }
        if changed == 0 {
            return Err(format!("Could not change {name} (administrator rights may be needed)"));
        }
        let state = app.state::<AppState>();
        let ledger = state.ledger.lock().map_err(err)?;
        ledger
            .record(&NewChange {
                category: "optimize".into(),
                title: format!("Efficiency mode {}: {name} ({changed} processes)", if on { "on" } else { "off" }),
                risk: Risk::Safe,
                status: ChangeStatus::Applied,
                bytes_freed: 0,
                items: u64::from(changed),
                details: serde_json::json!({ "name": name, "on": on, "note": "Lasts until the app closes" }),
                revert,
            })
            .map_err(err)?;
        Ok(changed)
    })
    .await
}

#[tauri::command]
pub fn open_windows_security() -> CmdResult<()> {
    #[cfg(windows)]
    {
        std::process::Command::new(sweeplark_core::platform::ops::system_exe("explorer.exe")).arg("windowsdefender:").spawn().map_err(err)?;
    }
    Ok(())
}

/// Start Sweeplark again as administrator (Windows asks with UAC) and open `page` there.
/// Settings and History are shared. If the user says No to UAC, this copy keeps running.
#[tauri::command]
pub async fn relaunch_as_admin(app: AppHandle, page: Option<String>) -> CmdResult<()> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(err)?;
        let page = page.filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase())).unwrap_or_else(|| "home".into());
        let pid = std::process::id();
        // Each piece is passed as its own PowerShell string (single quotes doubled inside).
        let q = |s: &str| format!("'{}'", s.replace('\'', "''").replace(['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'], "''"));
        let script = format!(
            "$ErrorActionPreference='Stop'; Start-Process -FilePath {} -Verb RunAs -ArgumentList {},{}",
            q(&exe.display().to_string()),
            q(&format!("--page={page}")),
            q(&format!("--wait-parent={pid}"))
        );
        let out = blocking(move || {
            sweeplark_core::platform::ops::run_timeout(
                "powershell",
                &["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script],
                std::time::Duration::from_secs(300),
            )
            .map_err(err)
        })
        .await?;
        if !out.status.success() {
            sweeplark_core::log::info("restart as administrator cancelled");
            return Err("Administrator permission was not given.".into());
        }
        sweeplark_core::log::info("restarting as administrator");
        crate::quit(&app);
    }
    #[cfg(not(windows))]
    let _ = (app, page);
    Ok(())
}

// ---- updates ----

use sweeplark_core::update::{self, UpdateInfo};
use tauri_plugin_notification::NotificationExt;

/// Check once shortly after start (and every 12 h while running).
pub fn start_update_check(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(15));
        loop {
            let s = settings_of(&app);
            // Offline at sign-in is common: try again in 30 minutes instead of 12 hours.
            let mut wait = std::time::Duration::from_secs(12 * 3600);
            let result = if s.check_updates { update::check(env!("CARGO_PKG_VERSION")) } else { Ok(None) };
            if let Err(e) = &result {
                sweeplark_core::log::info(&format!("update check failed, retry in 30 min: {e}"));
                wait = std::time::Duration::from_secs(30 * 60);
            }
            if let Ok(Some(info)) = result
                && s.skipped_version.as_deref() != Some(info.version.as_str())
            {
                let first = app.state::<AppState>().update.lock().map(|g| g.is_none()).unwrap_or(false);
                if let Ok(mut g) = app.state::<AppState>().update.lock() {
                    *g = Some(info.clone());
                }
                if first {
                    let (title, body) = match s.language {
                        sweeplark_core::Lang::En => (format!("Sweeplark {} is available", info.version), "Open Sweeplark to update.".to_string()),
                        sweeplark_core::Lang::Hinglish => (format!("Sweeplark {} aa gaya hai", info.version), "Update karne ke liye Sweeplark kholein.".to_string()),
                    };
                    let _ = app.notification().builder().title(title).body(body).show();
                    let _ = app.emit("update-available", info);
                }
            }
            std::thread::sleep(wait);
        }
    });
}

/// `manual = true` ignores a skipped version (the "Check for updates" button).
#[tauri::command]
pub async fn update_check(app: AppHandle, manual: bool) -> CmdResult<Option<UpdateInfo>> {
    blocking(move || {
        if !manual
            && let Some(u) = app.state::<AppState>().update.lock().map_err(err)?.clone()
        {
            return Ok(Some(u));
        }
        let info = update::check(env!("CARGO_PKG_VERSION")).map_err(err)?;
        let s = settings_of(&app);
        let info = info.filter(|i| manual || s.skipped_version.as_deref() != Some(i.version.as_str()));
        *app.state::<AppState>().update.lock().map_err(err)? = info.clone();
        Ok(info)
    })
    .await
}

#[tauri::command]
pub async fn update_skip(app: AppHandle, version: String) -> CmdResult<()> {
    blocking(move || update_skip_inner(&app, version)).await
}

fn update_skip_inner(app: &AppHandle, version: String) -> CmdResult<()> {
    update_settings(app, |s| s.skipped_version = Some(version));
    *app.state::<AppState>().update.lock().map_err(err)? = None;
    Ok(())
}

/// Download the new installer (hash-checked), start it and close Sweeplark.
/// Portable copies just open the release page.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> CmdResult<()> {
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("The update is already downloading".into());
    }
    let r = update_install_inner(app).await;
    RUNNING.store(false, Ordering::SeqCst);
    r
}

async fn update_install_inner(app: AppHandle) -> CmdResult<()> {
    let info = app.state::<AppState>().update.lock().map_err(err)?.clone().ok_or("No update")?;
    if sweeplark_core::paths::AppPaths::is_portable() || info.setup_url.is_none() {
        return open_url(info.page_url.clone());
    }
    let dir = app.state::<AppState>().paths.data_dir.join("updates");
    // Old installers from earlier updates are not needed any more.
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if !e.file_name().to_string_lossy().contains(&info.version) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let app2 = app.clone();
    let path = blocking(move || {
        let mut last = 0u64;
        update::download_setup(&info, &dir, &mut |done, total| {
            if done - last > 256 * 1024 || done == total {
                last = done;
                let _ = app2.emit("update-progress", (done, total));
            }
            true
        })
        .map_err(err)
    })
    .await;
    let path = logged("update download", path)?;
    sweeplark_core::log::info(&format!("installing update {}", info_version(&app)));
    std::process::Command::new(path).arg("/P").arg("/R").spawn().map_err(err)?;
    crate::quit(&app);
    Ok(())
}

fn info_version(app: &AppHandle) -> String {
    app.state::<AppState>().update.lock().ok().and_then(|g| g.as_ref().map(|i| i.version.clone())).unwrap_or_default()
}

/// Open an https link in the default browser (About section, release page).
#[tauri::command]
pub fn open_url(url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("Only https links can be opened".into());
    }
    #[cfg(windows)]
    std::process::Command::new(sweeplark_core::platform::ops::system_exe("explorer.exe")).arg(&url).spawn().map_err(err)?;
    Ok(())
}

/// The page the window should open on (set by the tray before the window existed).
#[tauri::command]
pub fn take_pending_page(state: State<'_, AppState>) -> Option<String> {
    state.pending_page.lock().ok().and_then(|mut g| g.take())
}

/// Open the folder with Sweeplark's log files.
#[tauri::command]
pub fn logs_open() -> CmdResult<()> {
    let dir = sweeplark_core::log::dir().ok_or("No log folder yet")?;
    #[cfg(windows)]
    std::process::Command::new(sweeplark_core::platform::ops::system_exe("explorer.exe")).arg(&dir).spawn().map_err(err)?;
    let _ = dir;
    Ok(())
}

/// Text for a bug report: version, Windows version and the last log lines (no personal data:
/// user-folder paths are shortened to %USERPROFILE%).
#[tauri::command]
pub async fn bug_report() -> CmdResult<String> {
    blocking(|| {
        let os = sysinfo::System::long_os_version().unwrap_or_else(|_| std::env::consts::OS.into());
        let mut log = sweeplark_core::log::tail(40);
        if let Some(home) = directories::UserDirs::new().map(|u| u.home_dir().display().to_string())
            && !home.is_empty()
        {
            log = log.replace(&home, "%USERPROFILE%");
        }
        Ok(format!(
            "Sweeplark {}{}\nWindows: {os}\nAdmin: {}\n\nWhat happened:\n\n\nLast log lines:\n```\n{log}\n```",
            env!("CARGO_PKG_VERSION"),
            if sweeplark_core::paths::AppPaths::is_portable() { " (portable)" } else { "" },
            system::is_admin()
        ))
    })
    .await
}
