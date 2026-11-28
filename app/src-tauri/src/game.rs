//! Game mode: when a full-screen app is in front (a game), heavy background apps are put in
//! Efficiency mode, and put back when the game closes. RAM guard: frees RAM from background
//! apps when memory stays nearly full. Both are off by default (Boost page / Settings).

use std::time::Duration;

use sweeplark_core::boost::{self, BoostOptions};
use sweeplark_core::ledger::{ChangeStatus, NewChange};
use sweeplark_core::revert::RevertStep;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

struct ActiveGame {
    pid: u32,
    /// Program name, so a reused process id is not mistaken for the game.
    name: String,
    /// What to put back, kept here (not only in History) so it works even if the database
    /// could not record it.
    revert: Vec<RevertStep>,
    ledger_id: Option<i64>,
}

#[derive(Default)]
pub struct GameState {
    active: Option<ActiveGame>,
    /// RAM guard: checks in a row above the limit, and when it last acted.
    high_ram: u32,
    last_guard: Option<std::time::Instant>,
}

fn game_still_running(g: &ActiveGame) -> bool {
    use sweeplark_core::platform::system;
    if !system::process_alive(g.pid) {
        return false;
    }
    // Can't read the name (elevated game): trust "alive". Different name: the id was reused.
    system::process_name(g.pid).is_none_or(|n| n.eq_ignore_ascii_case(&g.name))
}

/// RAM guard: RAM above the limit for 4 checks (~1 minute) frees RAM from background apps.
fn ram_guard(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (on, limit, never, lang) = match state.settings.lock() {
        Ok(s) => (s.boost.ram_guard, s.boost.ram_guard_percent, s.boost.never.clone(), s.language),
        Err(_) => return,
    };
    if !on {
        return;
    }
    let Ok(mut m) = state.boost_monitor.lock() else { return };
    let st = m.sample();
    let pct = st.ram_used as f64 / st.ram_total.max(1) as f64 * 100.0;
    let Ok(mut g) = state.game.lock() else { return };
    g.high_ram = if pct >= f64::from(limit) { g.high_ram + 1 } else { 0 };
    let cooled = g.last_guard.is_none_or(|t| t.elapsed() >= Duration::from_secs(600));
    if g.high_ram < 4 || !cooled {
        return;
    }
    g.high_ram = 0;
    g.last_guard = Some(std::time::Instant::now());
    drop(g);
    let (freed, trimmed) = boost::ram_guard(&mut m, &never);
    drop(m);
    sweeplark_core::log::info(&format!("RAM guard at {pct:.0}%: freed {freed} bytes from {trimmed} processes"));
    if freed > 200 * 1024 * 1024 {
        let size = sweeplark_core::util::format_bytes(freed);
        let body = match lang {
            sweeplark_core::Lang::En => format!("RAM was {pct:.0}% full. Freed {size} from background apps."),
            sweeplark_core::Lang::Hinglish => format!("RAM {pct:.0}% bhari thi. Background apps se {size} khaali ki."),
        };
        let _ = app.notification().builder().title("Sweeplark · RAM guard").body(body).show();
    }
}

fn restore(app: &AppHandle, g: ActiveGame) {
    let state = app.state::<AppState>();
    let reg = sweeplark_core::platform::registry::system_registry();
    let _ = sweeplark_core::revert::apply_all(&g.revert, reg.as_ref());
    // Mark it done in History too (putting back twice is harmless).
    if let (Some(id), Ok(l)) = (g.ledger_id, state.ledger.lock()) {
        let _ = l.undo(id, reg.as_ref());
    }
    sweeplark_core::log::info(&format!("game mode: {} closed, apps restored", g.name));
}

fn check(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (enabled, never, lang) = match state.settings.lock() {
        Ok(s) => (s.boost.auto_game, s.boost.never.clone(), s.language),
        Err(_) => return,
    };
    // Game closed (or game mode switched off): put everything back.
    let finished = state.game.lock().ok().and_then(|mut g| {
        let done = g.active.as_ref().is_some_and(|a| !enabled || !game_still_running(a));
        if done { g.active.take() } else { None }
    });
    if let Some(g) = finished {
        restore(app, g);
        return;
    }
    if !enabled || state.game.lock().map(|g| g.active.is_some()).unwrap_or(true) {
        return;
    }
    let Some(pid) = sweeplark_core::platform::system::fullscreen_foreground() else { return };
    let game = sweeplark_core::platform::system::process_name(pid).unwrap_or_else(|| "game".into());
    let opts = BoostOptions { eco_cpu: true, eco_gpu: true, ..Default::default() };
    let report = match state.boost_monitor.lock() {
        Ok(mut m) => boost::apply(&mut m, &opts, &never, Some(pid)),
        Err(_) => return,
    };
    let revert = report.revert_steps();
    let ledger_id = if revert.is_empty() {
        None
    } else {
        state.ledger.lock().ok().and_then(|l| {
            l.record(&NewChange {
                category: "boost".into(),
                title: format!("{}: {game} ({} apps calmed)", lang.pick("Game mode", "Game mode"), report.eco.len()),
                risk: sweeplark_core::Risk::Safe,
                status: ChangeStatus::Applied,
                bytes_freed: 0,
                items: report.eco.len() as u64,
                details: serde_json::to_value(&report).unwrap_or_default(),
                revert: revert.clone(),
            })
            .ok()
        })
    };
    // Even with nothing calmed, remember the game so it is not checked again and again.
    if let Ok(mut g) = state.game.lock() {
        g.active = Some(ActiveGame { pid, name: game.clone(), revert, ledger_id });
    }
    if !report.eco.is_empty() {
        let body = match lang {
            sweeplark_core::Lang::En => format!("Calmed {} background apps for {game}. They go back to normal when you close it.", report.eco.len()),
            sweeplark_core::Lang::Hinglish => format!("{game} ke liye {} background apps shaant ki. Game band karte hi normal ho jayengi.", report.eco.len()),
        };
        let _ = app.notification().builder().title("Sweeplark · Game mode").body(body).show();
    }
}

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("sweeplark-game".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(15));
                check(&app);
                ram_guard(&app);
            }
        })
        .expect("game thread");
}
