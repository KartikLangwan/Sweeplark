use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Human readable size, e.g. `1.4 GB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Windows FILETIME (100ns ticks since 1601-01-01) to `SystemTime`.
pub fn filetime_to_system_time(ticks: u64) -> Option<SystemTime> {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let secs = ticks / 10_000_000;
    let nanos = (ticks % 10_000_000) * 100;
    let unix_secs = secs.checked_sub(EPOCH_DIFF_SECS)?;
    Some(UNIX_EPOCH + Duration::new(unix_secs, nanos as u32))
}

/// `SystemTime` to Windows FILETIME ticks.
pub fn system_time_to_filetime(time: SystemTime) -> u64 {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    (since.as_secs() + EPOCH_DIFF_SECS) * 10_000_000 + u64::from(since.subsec_nanos()) / 100
}

pub fn age_hours(time: SystemTime, now: SystemTime) -> u64 {
    now.duration_since(time).map(|d| d.as_secs() / 3600).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
        assert_eq!(format_bytes(250 * 1024 * 1024), "250 MB");
    }

    #[test]
    fn filetime_roundtrip() {
        let now = SystemTime::now();
        let ft = system_time_to_filetime(now);
        let back = filetime_to_system_time(ft).unwrap();
        let diff = now.duration_since(back).unwrap_or_else(|e| e.duration());
        assert!(diff < Duration::from_micros(1));
    }
}
