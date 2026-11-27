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

    /// Windows 11 "Efficiency mode" for one process: low priority + EcoQoS power throttling.
    /// It lasts until the process exits; turning it off restores normal priority.
    pub fn set_efficiency_mode(pid: u32, on: bool) -> crate::Result<()> {
        use windows::Win32::System::Threading::{
            IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE, PROCESS_SET_INFORMATION,
            ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_SET_INFORMATION, false, pid)
                .map_err(|e| crate::Error::NeedsAdmin(format!("process {pid}: {e}")))?;
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            };
            let r1 = SetProcessInformation(
                handle,
                ProcessPowerThrottling,
                (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(),
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            let r2 = SetPriorityClass(handle, if on { IDLE_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS });
            let _ = CloseHandle(handle);
            r1.and(r2).map_err(|e| crate::Error::Other(format!("could not change process {pid}: {e}")))
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
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut sid_str = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut sid_str).ok()?;
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
