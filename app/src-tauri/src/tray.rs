//! System tray: live stats on hover, quick actions on right-click, amber dot when
//! something waits for the user.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sweeplark_core::Lang;
use sysinfo::{Components, CpuRefreshKind, Disks, Gpus, MemoryRefreshKind, RefreshKind, System};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::{AppState, show_main};

static ATTENTION: AtomicBool = AtomicBool::new(false);

fn lang(app: &AppHandle) -> Lang {
    app.state::<AppState>().settings.lock().map(|s| s.language).unwrap_or_default()
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let l = lang(app);
    let item = |id: &str, en: &str, hi: &str| MenuItem::with_id(app, id, l.pick(en, hi), true, None::<&str>);
    let autostart_on = app.state::<AppState>().settings.lock().map(|s| s.agent.start_with_windows).unwrap_or(true);
    let menu = Menu::with_items(
        app,
        &[
            &item("open", "Open Sweeplark", "Sweeplark kholein")?,
            &item("home", "One-Click Optimize", "One-Click Optimize")?,
            &PredefinedMenuItem::separator(app)?,
            &item("clean", "Clean", "Safai")?,
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
            "quit" => {
                app.state::<AppState>().ai.unload();
                app.exit(0);
            }
            "autostart" => {
                let state = app.state::<AppState>();
                if let Ok(mut s) = state.settings.lock() {
                    s.agent.start_with_windows = !s.agent.start_with_windows;
                    let _ = s.save(&state.paths.settings_file());
                }
                crate::commands::apply_autostart(app);
            }
            page => show_main(app, Some(page)),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle(), None);
            }
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
    }
}

fn temp(components: &Components, words: &[&str]) -> Option<f32> {
    components
        .iter()
        .filter(|c| {
            let l = c.label().to_lowercase();
            words.iter().any(|w| l.contains(w))
        })
        .filter_map(|c| c.temperature())
        .filter(|t| *t > 0.0 && *t < 130.0)
        .reduce(f32::max)
}

/// Hover text, refreshed every few seconds with a light sysinfo refresh.
pub fn start_stats(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(mut sys) = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_usage()).with_memory(MemoryRefreshKind::nothing().with_ram()),
        ) else {
            return;
        };
        let mut components = Components::new_with_refreshed_list().ok();
        let mut gpus = Gpus::new_with_refreshed_list().ok();
        let mut disks = Disks::new_with_refreshed_list().ok();
        let sys_mount = format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())).to_lowercase();
        loop {
            std::thread::sleep(Duration::from_secs(4));
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            if let Some(c) = components.as_mut() {
                c.refresh(false);
            }
            if let Some(g) = gpus.as_mut() {
                g.refresh(false);
            }
            if let Some(d) = disks.as_mut() {
                d.refresh(false);
            }
            let l = lang(&app);
            let deg = |t: Option<f32>| t.map(|t| format!(" · {t:.0}°C")).unwrap_or_default();
            let cpu_t = components.as_ref().and_then(|c| temp(c, &["cpu", "package", "core", "tctl", "acpi"]));
            let gpu_t = components.as_ref().and_then(|c| temp(c, &["gpu", "nvidia", "radeon", "amdgpu"]));
            let gpu_use = gpus.as_ref().and_then(|g| g.iter().filter_map(|g| g.usage()).reduce(f32::max));
            let (used, total) = (sys.used_memory() as f64, sys.total_memory().max(1) as f64);
            let gb = 1024f64.powi(3);
            let mut lines = vec![
                if ATTENTION.load(Ordering::Relaxed) { l.pick("Sweeplark · approval needed", "Sweeplark · ijazat chahiye") } else { "Sweeplark" }.to_string(),
                format!("CPU {:.0}%{}", sys.global_cpu_usage(), deg(cpu_t)),
            ];
            if gpu_use.is_some() || gpu_t.is_some() {
                lines.push(format!("GPU {}{}", gpu_use.map(|u| format!("{u:.0}%")).unwrap_or_else(|| "–".into()), deg(gpu_t)));
            }
            lines.push(format!("RAM {:.1}/{:.0} GB ({:.0}%)", used / gb, total / gb, used / total * 100.0));
            if let Some(d) = disks.as_ref().and_then(|d| d.iter().find(|d| d.mount_point().to_string_lossy().to_lowercase() == sys_mount)) {
                lines.push(format!("{} {:.0} GB {}", sys_mount.trim_end_matches('\\').to_uppercase(), d.available_space() as f64 / gb, l.pick("free", "khaali")));
            }
            // Windows cuts tray tooltips at 127 characters.
            let mut text = lines.join("\n");
            text.truncate(127);
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_tooltip(Some(text));
            }
        }
    });
}
