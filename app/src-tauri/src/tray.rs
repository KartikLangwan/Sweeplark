//! System tray: live stats on hover, quick actions on right-click, amber dot when
//! something waits for the user. Stats are read only when the mouse is over the icon, so
//! an idle Sweeplark costs no CPU.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sweeplark_core::Lang;
use sysinfo::Disks;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::{AppState, show_main};

static ATTENTION: AtomicBool = AtomicBool::new(false);
static LAST_HOVER: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);

fn lang(app: &AppHandle) -> Lang {
    app.state::<AppState>().settings.lock().map(|s| s.language).unwrap_or_default()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let l = lang(app);
    let item = |id: &str, en: &str, hi: &str| MenuItem::with_id(app, id, l.pick(en, hi), true, None::<&str>);
    let autostart_on = app.state::<AppState>().settings.lock().map(|s| s.agent.start_with_windows).unwrap_or(true);
    let menu = Menu::with_items(
        app,
        &[
            &item("open", "Open Sweeplark", "Sweeplark kholein")?,
            &item("boost", "Boost now", "Abhi Boost")?,
            &item("home", "One-Click Optimize", "One-Click Optimize")?,
            &PredefinedMenuItem::separator(app)?,
            &item("clean", "Clean", "Safai")?,
            &item("optimizer", "Optimizer", "Optimizer")?,
            &item("startup", "Startup apps", "Startup apps")?,
            &item("health", "Health", "Sehat")?,
            &item("assistant", "Assistant", "Assistant")?,
            &item("inbox", "Messages", "Messages")?,
            &item("settings", "Settings", "Settings")?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(app, "autostart", l.pick("Start with Windows", "Windows ke saath chalu"), true, autostart_on, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", "Quit", "Band karein")?,
        ],
    )?;
    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?)
        .tooltip("Sweeplark")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app, None),
            "quit" => crate::quit(app),
            "autostart" => {
                let state = app.state::<AppState>();
                if let Ok(mut s) = state.settings.lock() {
                    s.agent.start_with_windows = !s.agent.start_with_windows;
                    let _ = s.save(&state.paths.settings_file());
                }
                crate::commands::apply_autostart(app);
                crate::settings_changed(app);
            }
            page => show_main(app, Some(page)),
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => {
                show_main(tray.app_handle(), None);
            }
            TrayIconEvent::Enter { .. } | TrayIconEvent::Move { .. } => {
                let last = LAST_HOVER.load(Ordering::Relaxed);
                if now_ms().saturating_sub(last) > 2000 && !BUSY.swap(true, Ordering::Relaxed) {
                    LAST_HOVER.store(now_ms(), Ordering::Relaxed);
                    let app = tray.app_handle().clone();
                    std::thread::spawn(move || {
                        update_tooltip(&app);
                        BUSY.store(false, Ordering::Relaxed);
                    });
                }
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Swap the icon when an approval is pending.
pub fn set_attention(app: &AppHandle, attention: bool) {
    if ATTENTION.swap(attention, Ordering::Relaxed) == attention {
        return;
    }
    if let Some(tray) = app.tray_by_id("main") {
        let bytes: &[u8] =
            if attention { include_bytes!("../icons/tray-alert@2x.png") } else { include_bytes!("../icons/tray@2x.png") };
        if let Ok(img) = tauri::image::Image::from_bytes(bytes) {
            let _ = tray.set_icon(Some(img));
        }
        let _ = tray.set_tooltip(Some(if attention { "Sweeplark · approval needed" } else { "Sweeplark" }));
    }
}

fn update_tooltip(app: &AppHandle) {
    let Some(st) = app.state::<AppState>().monitor.lock().ok().map(|mut m| m.sample()) else { return };
    let l = lang(app);
    let deg = |t: Option<f32>| t.map(|t| format!(" · {t:.0}°C")).unwrap_or_default();
    let gb = 1024f64.powi(3);
    let mut lines = vec![
        if ATTENTION.load(Ordering::Relaxed) { l.pick("Sweeplark · approval needed", "Sweeplark · ijazat chahiye") } else { "Sweeplark" }.to_string(),
        format!("CPU {:.0}%{}", st.cpu_percent, deg(st.cpu_temp)),
    ];
    if st.gpu_percent.is_some() || st.gpu_temp.is_some() {
        lines.push(format!("GPU {}{}", st.gpu_percent.map(|u| format!("{u:.0}%")).unwrap_or_else(|| "–".into()), deg(st.gpu_temp)));
    }
    let (used, total) = (st.ram_used as f64, st.ram_total.max(1) as f64);
    lines.push(format!("RAM {:.1}/{:.0} GB ({:.0}%)", used / gb, total / gb, used / total * 100.0));
    let sys_mount = format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())).to_lowercase();
    if let Ok(disks) = Disks::new_with_refreshed_list()
        && let Some(d) = disks.iter().find(|d| d.mount_point().to_string_lossy().to_lowercase() == sys_mount)
    {
        lines.push(format!("{} {:.0} GB {}", sys_mount.trim_end_matches('\\').to_uppercase(), d.available_space() as f64 / gb, l.pick("free", "khaali")));
    }
    let mut text = lines.join("\n");
    text.truncate(127);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}
