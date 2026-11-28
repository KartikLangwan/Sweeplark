//! Small OS queries: admin rights, user SID, recycle bin, security and hardware status.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct SecurityStatus {
    /// `None` = could not be read (third party antivirus, or no access).
    pub defender_antivirus_enabled: Option<bool>,
    pub defender_realtime_enabled: Option<bool>,
    pub defender_signature_age_days: Option<u32>,
    /// Firewall profile name ("Domain", "Private", "Public") and whether it is on.
    pub firewall_profiles: Vec<(String, bool)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhysicalDiskHealth {
    pub name: String,
    /// "SSD", "HDD" or "Unknown".
    pub media: String,
    /// "Healthy", "Warning", "Unhealthy" or "Unknown".
    pub health: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatteryStatus {
    pub percent: Option<u8>,
    pub charging: Option<bool>,
    pub on_ac_power: Option<bool>,
    pub design_capacity_mwh: Option<u64>,
    pub full_charge_capacity_mwh: Option<u64>,
}

impl BatteryStatus {
    /// Battery wear in percent (0 = like new).
    pub fn wear_percent(&self) -> Option<f64> {
        let design = self.design_capacity_mwh? as f64;
        let full = self.full_charge_capacity_mwh? as f64;
        if design <= 0.0 {
            return None;
        }
        Some(((1.0 - full / design) * 100.0).clamp(0.0, 100.0))
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct RecycleBinInfo {
    pub items: u64,
    pub bytes: u64,
}

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub use other::*;

#[cfg(not(windows))]
mod other {
    use super::*;

    pub fn is_admin() -> bool {
        false
    }
    pub fn current_user_sid() -> Option<String> {
        None
    }
    pub fn recycle_bin_info() -> Option<RecycleBinInfo> {
        None
    }
    pub fn empty_recycle_bin() -> crate::Result<()> {
        Err(crate::Error::NotSupported("recycle bin".into()))
    }
    pub fn security_status() -> Option<SecurityStatus> {
        None
    }
    pub fn physical_disks() -> Vec<PhysicalDiskHealth> {
        Vec::new()
    }
    pub fn battery() -> Option<BatteryStatus> {
        None
    }
    pub fn idle_seconds() -> u64 {
        0
    }
    pub fn set_efficiency_mode(_pid: u32, _on: bool) -> crate::Result<()> {
        Err(crate::Error::NotSupported("efficiency mode".into()))
    }
    pub fn foreground_pid() -> Option<u32> {
        None
    }
    pub fn trim_working_set(_pid: u32) -> bool {
        false
    }
    pub fn purge_standby_list() -> crate::Result<()> {
        Err(crate::Error::NotSupported("standby list".into()))
    }
    pub fn flush_modified_list() -> crate::Result<()> {
        Err(crate::Error::NotSupported("memory".into()))
    }
    pub fn to_recycle_bin(_path: &std::path::Path) -> crate::Result<()> {
        Err(crate::Error::NotSupported("recycle bin".into()))
    }
    pub fn has_recycle_bin(_path: &std::path::Path) -> bool {
        false
    }
    pub fn priority_class(_pid: u32) -> Option<u32> {
        None
    }
    pub fn set_priority_class(_pid: u32, _class: u32) -> crate::Result<()> {
        Err(crate::Error::NotSupported("priority".into()))
    }
    pub fn kill_pid(_pid: u32) -> bool {
        false
    }
    pub fn window_behind_own() -> Option<u32> {
        None
    }
    pub fn fullscreen_foreground() -> Option<u32> {
        None
    }
    pub fn process_name(_pid: u32) -> Option<String> {
        None
    }
    pub fn message_box(_title: &str, _text: &str) {}
    pub fn visible_window_pids() -> std::collections::HashSet<u32> {
        std::collections::HashSet::new()
    }
    pub fn process_alive(_pid: u32) -> bool {
        false
    }
    pub fn cpu_temp_sensor() -> Option<f32> {
        None
    }
    pub fn webview2_installed() -> bool {
        true
    }
}

#[cfg(windows)]
mod win {
    use std::collections::HashMap;

    use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::UI::Shell::{
        IsUserAnAdmin, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW,
        SHQUERYRBINFO, SHQueryRecycleBinW,
    };
    use windows::core::{PCWSTR, PWSTR};
    use wmi::{Variant, WMIConnection};

    use super::*;

    pub fn is_admin() -> bool {
        unsafe { IsUserAnAdmin().as_bool() }
    }

    /// Seconds since the last keyboard or mouse input.
    pub fn idle_seconds() -> u64 {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        unsafe {
            if !GetLastInputInfo(&mut info).as_bool() {
                return 0;
            }
            u64::from(GetTickCount().wrapping_sub(info.dwTime)) / 1000
        }
    }

    /// Process id of the window in front.
    pub fn foreground_pid() -> Option<u32> {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            (pid != 0).then_some(pid)
        }
    }

    /// Ask Windows to move a process's unused pages out of RAM (it pages them back on use).
    pub fn trim_working_set(pid: u32) -> bool {
        use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA};
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
            let ok = K32EmptyWorkingSet(h).as_bool();
            let _ = CloseHandle(h);
            ok
        }
    }

    /// Empty the standby list (cached file pages), like Sysinternals RAMMap. Needs admin.
    pub fn purge_standby_list() -> crate::Result<()> {
        memory_list_command(4)
    }

    /// Write changed pages to disk so their RAM becomes free ("flush modified list"). Admin.
    pub fn flush_modified_list() -> crate::Result<()> {
        memory_list_command(3)
    }

    /// 2 = empty working sets, 3 = flush modified list, 4 = purge standby, 5 = purge low-priority standby.
    fn memory_list_command(command: i32) -> crate::Result<()> {
        use windows::Win32::Security::{
            AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
            TOKEN_PRIVILEGES,
        };
        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn NtSetSystemInformation(class: i32, info: *const core::ffi::c_void, len: u32) -> i32;
        }
        const SYSTEM_MEMORY_LIST_INFORMATION: i32 = 80;
        if !is_admin() {
            return Err(crate::Error::NeedsAdmin("memory clean".into()));
        }
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token)
                .map_err(|e| crate::Error::Other(e.to_string()))?;
            let mut tp = TOKEN_PRIVILEGES { PrivilegeCount: 1, Privileges: [LUID_AND_ATTRIBUTES { Attributes: SE_PRIVILEGE_ENABLED, ..Default::default() }] };
            let name: Vec<u16> = "SeProfileSingleProcessPrivilege".encode_utf16().chain(std::iter::once(0)).collect();
            let _ = LookupPrivilegeValueW(PCWSTR::null(), PCWSTR(name.as_ptr()), &mut tp.Privileges[0].Luid);
            let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
            let _ = CloseHandle(token);
            let status = NtSetSystemInformation(SYSTEM_MEMORY_LIST_INFORMATION, (&command as *const i32).cast(), 4);
            if status >= 0 { Ok(()) } else { Err(crate::Error::Other(format!("memory clean failed (0x{status:08x})"))) }
        }
    }

    /// Windows 11 "Efficiency mode" for one process: low priority + EcoQoS power throttling.
    /// It lasts until the process exits. Turning it off hands throttling back to Windows
    /// and sets normal priority (callers that know the old priority restore that instead).
    pub fn set_efficiency_mode(pid: u32, on: bool) -> crate::Result<()> {
        use windows::Win32::System::Threading::{
            IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE, PROCESS_SET_INFORMATION,
            ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_SET_INFORMATION, false, pid)
                .map_err(|e| crate::Error::NeedsAdmin(format!("process {pid}: {e}")))?;
            // Off = ControlMask 0: let Windows decide again (not "never throttle").
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
                StateMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            };
            let r1 = SetProcessInformation(
                handle,
                ProcessPowerThrottling,
                (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(),
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            // Only touch the priority when throttling worked, so a failure never leaves a
            // process at idle priority without a record to undo it.
            let r = r1.and_then(|()| SetPriorityClass(handle, if on { IDLE_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS }));
            let _ = CloseHandle(handle);
            r.map_err(|e| crate::Error::Other(format!("could not change process {pid}: {e}")))
        }
    }

    /// Send a file or folder to the Recycle Bin.
    pub fn to_recycle_bin(path: &std::path::Path) -> crate::Result<()> {
        trash::delete(path).map_err(|e| crate::Error::Other(format!("could not move to the Recycle Bin: {e}")))
    }

    /// Fixed local drives have a Recycle Bin; USB sticks and network drives do not (Windows
    /// would delete for good there).
    pub fn has_recycle_bin(path: &std::path::Path) -> bool {
        use windows::Win32::Storage::FileSystem::GetDriveTypeW;
        let s = path.to_string_lossy();
        if s.starts_with(r"\\") {
            return false;
        }
        let Some(drive) = s.get(..2).filter(|d| d.ends_with(':')) else { return false };
        let root: Vec<u16> = format!("{drive}\\").encode_utf16().chain(std::iter::once(0)).collect();
        // 3 = DRIVE_FIXED
        unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) == 3 }
    }

    pub fn priority_class(pid: u32) -> Option<u32> {
        use windows::Win32::System::Threading::{GetPriorityClass, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let c = GetPriorityClass(h);
            let _ = CloseHandle(h);
            (c != 0).then_some(c)
        }
    }

    pub fn set_priority_class(pid: u32, class: u32) -> crate::Result<()> {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_CREATION_FLAGS, PROCESS_SET_INFORMATION, SetPriorityClass};
        unsafe {
            let h = OpenProcess(PROCESS_SET_INFORMATION, false, pid).map_err(|e| crate::Error::Other(format!("process {pid}: {e}")))?;
            let r = SetPriorityClass(h, PROCESS_CREATION_FLAGS(class));
            let _ = CloseHandle(h);
            r.map_err(|e| crate::Error::Other(format!("process {pid}: {e}")))
        }
    }

    /// End one process right away (used to stop our own AI engine without waiting).
    pub fn kill_pid(pid: u32) -> bool {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) else { return false };
            let ok = TerminateProcess(h, 1).is_ok();
            let _ = CloseHandle(h);
            ok
        }
    }

    fn is_app_window(hwnd: windows::Win32::Foundation::HWND) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongW, GetWindowTextLengthW, IsIconic, IsWindowVisible, WS_EX_TOOLWINDOW,
        };
        unsafe {
            IsWindowVisible(hwnd).as_bool()
                && !IsIconic(hwnd).as_bool()
                && GetWindowTextLengthW(hwnd) > 0
                && (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0) == 0
        }
    }

    fn pid_of(hwnd: windows::Win32::Foundation::HWND) -> u32 {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid
    }

    /// For a Store app the top-level window belongs to ApplicationFrameHost; the real app is
    /// the process of a child window. Returns that process (or `pid` unchanged).
    fn real_app_pid(hwnd: windows::Win32::Foundation::HWND, pid: u32) -> u32 {
        use windows::Win32::Foundation::LPARAM;
        use windows::core::BOOL;
        use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;
        if !process_name(pid).is_some_and(|n| n.eq_ignore_ascii_case("ApplicationFrameHost.exe")) {
            return pid;
        }
        unsafe extern "system" fn find(child: windows::Win32::Foundation::HWND, data: LPARAM) -> BOOL {
            let state = unsafe { &mut *(data.0 as *mut (u32, u32)) };
            let p = pid_of(child);
            if p != 0 && p != state.0 {
                state.1 = p;
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut state: (u32, u32) = (pid, 0);
        unsafe {
            let _ = EnumChildWindows(Some(hwnd), Some(find), LPARAM(&mut state as *mut (u32, u32) as isize));
        }
        if state.1 != 0 { state.1 } else { pid }
    }

    /// Processes that own a visible, normal app window (used to tell a game left running in
    /// the background from a background service).
    pub fn visible_window_pids() -> std::collections::HashSet<u32> {
        use windows::Win32::Foundation::LPARAM;
        use windows::core::BOOL;
        use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
        unsafe extern "system" fn add(hwnd: windows::Win32::Foundation::HWND, data: LPARAM) -> BOOL {
            let set = unsafe { &mut *(data.0 as *mut std::collections::HashSet<u32>) };
            if unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() } {
                let p = pid_of(hwnd);
                if p != 0 {
                    set.insert(real_app_pid(hwnd, p));
                }
            }
            BOOL(1)
        }
        let mut set = std::collections::HashSet::new();
        unsafe {
            let _ = EnumWindows(Some(add), LPARAM(&mut set as *mut _ as isize));
        }
        set
    }

    /// Is the process still running? (An elevated game we may not query still counts as running.)
    pub fn process_alive(pid: u32) -> bool {
        use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, WAIT_TIMEOUT};
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
        unsafe {
            match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                Ok(h) => {
                    let alive = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
                    let _ = CloseHandle(h);
                    alive
                }
                Err(e) => e.code() == ERROR_ACCESS_DENIED.to_hresult(),
            }
        }
    }

    /// The app the user was using: the window in front, or, when Sweeplark itself is in
    /// front (the user just clicked Boost), the next real app window behind it.
    pub fn window_behind_own() -> Option<u32> {
        use windows::Win32::UI::WindowsAndMessaging::{GW_HWNDNEXT, GetForegroundWindow, GetWindow};
        let own = std::process::id();
        unsafe {
            let mut hwnd = GetForegroundWindow();
            let mut steps = 0;
            while !hwnd.0.is_null() && steps < 400 {
                let pid = pid_of(hwnd);
                if pid != 0 && pid != own && is_app_window(hwnd) {
                    let name = process_name(pid).unwrap_or_default().to_lowercase();
                    // The desktop and taskbar are not "the app you are using".
                    if !matches!(name.as_str(), "explorer.exe" | "shellexperiencehost.exe" | "searchhost.exe" | "startmenuexperiencehost.exe" | "textinputhost.exe") {
                        return Some(real_app_pid(hwnd, pid));
                    }
                }
                hwnd = GetWindow(hwnd, GW_HWNDNEXT).unwrap_or_default();
                steps += 1;
            }
        }
        None
    }

    /// Process id of a full-screen app in front (a game or video), if any.
    pub fn fullscreen_foreground() -> Option<u32> {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetShellWindow, GetDesktopWindow, GetWindowRect};
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() || hwnd == GetShellWindow() || hwnd == GetDesktopWindow() {
                return None;
            }
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok()?;
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                return None;
            }
            let m = mi.rcMonitor;
            let covers = r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom;
            // A maximized normal window also covers the screen when the taskbar auto-hides;
            // games and full-screen video have no title bar and are not "maximized".
            use windows::Win32::UI::WindowsAndMessaging::{GWL_STYLE, GetWindowLongW, WS_CAPTION, WS_MAXIMIZE};
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let normal_window = (style & WS_CAPTION.0) == WS_CAPTION.0 || (style & WS_MAXIMIZE.0) != 0;
            let full = covers && !normal_window;
            let pid = real_app_pid(hwnd, pid_of(hwnd));
            if !full || pid == 0 || pid == std::process::id() {
                return None;
            }
            let name = process_name(pid).unwrap_or_default().to_lowercase();
            (!matches!(name.as_str(), "explorer.exe" | "dwm.exe" | "lockapp.exe")).then_some(pid)
        }
    }

    /// Real CPU temperature. Windows itself has no API for it (the "thermal zone" it offers
    /// is a motherboard sensor and often wrong), so this reads it from LibreHardwareMonitor or
    /// OpenHardwareMonitor when one of them is running. `None` otherwise.
    pub fn cpu_temp_sensor() -> Option<f32> {
        for ns in [r"ROOT\LibreHardwareMonitor", r"ROOT\OpenHardwareMonitor"] {
            let Some(rows) = query(ns, "SELECT Name, Value, Parent FROM Sensor WHERE SensorType='Temperature'") else { continue };
            let mut best: Option<(u8, f32)> = None;
            for r in &rows {
                let name = as_string(r.get("Name")).unwrap_or_default().to_lowercase();
                let parent = as_string(r.get("Parent")).unwrap_or_default().to_lowercase();
                if !parent.contains("cpu") && !name.contains("cpu") {
                    continue;
                }
                let value = match r.get("Value") {
                    Some(Variant::R4(v)) => *v,
                    Some(Variant::R8(v)) => *v as f32,
                    _ => continue,
                };
                if !(1.0..130.0).contains(&value) {
                    continue;
                }
                // Package / Tctl first, then the hottest core.
                let rank = if name.contains("package") || name.contains("tctl") || name.contains("tdie") { 0 } else if name.contains("core") { 1 } else { 2 };
                if best.is_none_or(|(br, bv)| rank < br || (rank == br && value > bv)) {
                    best = Some((rank, value));
                }
            }
            if let Some((_, v)) = best {
                return Some(v);
            }
        }
        None
    }

    /// A plain Windows message box (used before any window exists).
    pub fn message_box(title: &str, text: &str) {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
        let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let m: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(None, PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), MB_OK | MB_ICONWARNING);
        }
    }

    /// Whether the Microsoft Edge WebView2 runtime (needed for the window) is installed.
    pub fn webview2_installed() -> bool {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        const ID: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
        let keys = [
            (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\{ID}")),
            (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{ID}")),
            (HKEY_CURRENT_USER, format!(r"Software\{ID}")),
        ];
        keys.iter().any(|(hive, key)| {
            RegKey::predef(*hive)
                .open_subkey(key)
                .and_then(|k| k.get_value::<String, _>("pv"))
                .is_ok_and(|v| !v.is_empty() && v != "0.0.0.0")
        })
    }

    /// Executable name of a process (e.g. "chrome.exe").
    pub fn process_name(pid: u32) -> Option<String> {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            if !ok {
                return None;
            }
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            full.rsplit('\\').next().map(String::from)
        }
    }

    pub fn current_user_sid() -> Option<String> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
            let mut len = 0u32;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            let ok = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len);
            let _ = CloseHandle(token);
            ok.ok()?;
            // The byte buffer may not be aligned for TOKEN_USER: copy it out.
            let user = std::ptr::read_unaligned(buf.as_ptr() as *const TOKEN_USER);
            let mut sid_str = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut sid_str).ok()?;
            drop(buf);
            let s = sid_str.to_string().ok();
            let _ = LocalFree(Some(HLOCAL(sid_str.0.cast())));
            s
        }
    }

    pub fn recycle_bin_info() -> Option<RecycleBinInfo> {
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
        unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info).ok()? };
        Some(RecycleBinInfo { items: info.i64NumItems.max(0) as u64, bytes: info.i64Size.max(0) as u64 })
    }

    pub fn empty_recycle_bin() -> crate::Result<()> {
        unsafe {
            SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)
                .map_err(|e| crate::Error::Other(format!("could not empty the recycle bin: {e}")))
        }
    }

    type Row = HashMap<String, Variant>;

    fn query(namespace: &str, wql: &str) -> Option<Vec<Row>> {
        let con = WMIConnection::with_namespace_path(namespace).ok()?;
        con.raw_query::<Row>(wql).ok()
    }

    fn as_u64(v: Option<&Variant>) -> Option<u64> {
        match v? {
            Variant::UI1(x) => Some(u64::from(*x)),
            Variant::UI2(x) => Some(u64::from(*x)),
            Variant::UI4(x) => Some(u64::from(*x)),
            Variant::UI8(x) => Some(*x),
            Variant::I1(x) => u64::try_from(*x).ok(),
            Variant::I2(x) => u64::try_from(*x).ok(),
            Variant::I4(x) => u64::try_from(*x).ok(),
            Variant::I8(x) => u64::try_from(*x).ok(),
            Variant::String(s) => s.parse().ok(),
            Variant::Bool(b) => Some(u64::from(*b)),
            _ => None,
        }
    }

    fn as_bool(v: Option<&Variant>) -> Option<bool> {
        match v? {
            Variant::Bool(b) => Some(*b),
            other => as_u64(Some(other)).map(|n| n == 1),
        }
    }

    fn as_string(v: Option<&Variant>) -> Option<String> {
        match v? {
            Variant::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn security_status() -> Option<SecurityStatus> {
        let mut status = SecurityStatus::default();
        if let Some(rows) = query(
            r"ROOT\Microsoft\Windows\Defender",
            "SELECT AntivirusEnabled, RealTimeProtectionEnabled, AntivirusSignatureAge FROM MSFT_MpComputerStatus",
        ) && let Some(row) = rows.first()
        {
            status.defender_antivirus_enabled = as_bool(row.get("AntivirusEnabled"));
            status.defender_realtime_enabled = as_bool(row.get("RealTimeProtectionEnabled"));
            status.defender_signature_age_days =
                as_u64(row.get("AntivirusSignatureAge")).and_then(|v| u32::try_from(v).ok());
        }
        if let Some(rows) = query(r"ROOT\StandardCimv2", "SELECT Name, Enabled FROM MSFT_NetFirewallProfile") {
            for row in rows {
                if let (Some(name), Some(enabled)) = (as_string(row.get("Name")), as_u64(row.get("Enabled"))) {
                    status.firewall_profiles.push((name, enabled == 1));
                }
            }
        }
        Some(status)
    }

    pub fn physical_disks() -> Vec<PhysicalDiskHealth> {
        let Some(rows) = query(
            r"ROOT\Microsoft\Windows\Storage",
            "SELECT FriendlyName, MediaType, HealthStatus, Size FROM MSFT_PhysicalDisk",
        ) else {
            return Vec::new();
        };
        rows.iter()
            .map(|row| PhysicalDiskHealth {
                name: as_string(row.get("FriendlyName")).unwrap_or_else(|| "Disk".into()),
                media: match as_u64(row.get("MediaType")) {
                    Some(3) => "HDD",
                    Some(4) => "SSD",
                    Some(5) => "SCM",
                    _ => "Unknown",
                }
                .into(),
                health: match as_u64(row.get("HealthStatus")) {
                    Some(0) => "Healthy",
                    Some(1) => "Warning",
                    Some(2) => "Unhealthy",
                    _ => "Unknown",
                }
                .into(),
                size_bytes: as_u64(row.get("Size")).unwrap_or(0),
            })
            .collect()
    }

    pub fn battery() -> Option<BatteryStatus> {
        let mut ps = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut ps).ok()? };
        // BatteryFlag 128 = no system battery.
        if ps.BatteryFlag == 128 {
            return None;
        }
        let percent = (ps.BatteryLifePercent <= 100).then_some(ps.BatteryLifePercent);
        let charging = (ps.BatteryFlag != 255).then_some(ps.BatteryFlag & 8 != 0);
        let on_ac_power = match ps.ACLineStatus {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        let design = query(r"ROOT\WMI", "SELECT DesignedCapacity FROM BatteryStaticData")
            .and_then(|rows| rows.first().and_then(|r| as_u64(r.get("DesignedCapacity"))));
        let full = query(r"ROOT\WMI", "SELECT FullChargedCapacity FROM BatteryFullChargedCapacity")
            .and_then(|rows| rows.first().and_then(|r| as_u64(r.get("FullChargedCapacity"))));
        Some(BatteryStatus {
            percent,
            charging,
            on_ac_power,
            design_capacity_mwh: design,
            full_charge_capacity_mwh: full,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_wear() {
        let b = BatteryStatus {
            percent: Some(80),
            charging: Some(false),
            on_ac_power: Some(false),
            design_capacity_mwh: Some(50_000),
            full_charge_capacity_mwh: Some(40_000),
        };
        assert!((b.wear_percent().unwrap() - 20.0).abs() < 0.01);
    }
}
