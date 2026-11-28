//! Game mode: when a full-screen app is in front (a game), heavy background apps are put in
//! Efficiency mode, and put back when the game closes. Off by default (Settings > Boost).

use std::time::Duration;

use sweeplark_core::boost::{self, BoostOptions};
use sweeplark_core::ledger::{ChangeStatus, NewChange};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

#[derive(Default)]
pub struct GameState {
    /// Game process and the ledger entry that puts the calmed apps back (None = nothing calmed).
    active: Option<(u32, Option<i64>)>,
    /// RAM guard: checks in a row above the limit, and when it last acted.
    high_ram: u32,
    last_guard: Option<std::time::Instant>,
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
    let Ok(mut m) = state.monitor.lock() else { return };
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

fn check(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (enabled, never, lang) = match state.settings.lock() {
        Ok(s) => (s.boost.auto_game, s.boost.never.clone(), s.language),
        Err(_) => return,
    };
    let active = state.game.lock().ok().and_then(|g| g.active);
    // Game closed (or game mode switched off): put everything back.
    if let Some((pid, ledger_id)) = active {
        let gone = sweeplark_core::platform::system::process_name(pid).is_none();
        if gone || !enabled {
            if let (Some(id), Ok(l)) = (ledger_id, state.ledger.lock()) {
                let _ = l.undo(id, sweeplark_core::platform::registry::system_registry().as_ref());
            }
            if let Ok(mut g) = state.game.lock() {
                g.active = None;
            }
            sweeplark_core::log::info("game mode: game closed, apps restored");
        }
        return;
    }
    if !enabled {
        return;
    }
    let Some(pid) = sweeplark_core::platform::system::fullscreen_foreground() else { return };
    let opts = BoostOptions { eco_cpu: true, eco_gpu: true, ..Default::default() };
    let report = match state.monitor.lock() {
        Ok(mut m) => boost::apply(&mut m, &opts, &never, Some(pid)),
        Err(_) => return,
    };
    let steps = report.revert_steps();
    let game = sweeplark_core::platform::system::process_name(pid).unwrap_or_else(|| "game".into());
    let id = if steps.is_empty() { None } else { state.ledger.lock().ok().and_then(|l| {
        l.record(&NewChange {
            category: "boost".into(),
            title: lang.pick("Game mode", "Game mode").to_string() + &format!(": {game} ({} apps calmed)", report.eco.len()),
            risk: sweeplark_core::Risk::Safe,
            status: ChangeStatus::Applied,
            bytes_freed: 0,
            items: report.eco.len() as u64,
            details: serde_json::to_value(&report).unwrap_or_default(),
            revert: steps.clone(),
        })
        .ok()
    }) };
    // Even with nothing calmed, remember the game so it is not checked again and again.
    if let Ok(mut g) = state.game.lock() {
        g.active = Some((pid, id));
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
