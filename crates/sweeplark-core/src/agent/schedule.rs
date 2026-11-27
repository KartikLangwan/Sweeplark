//! When is a scheduled job due? Works in local time and catches up if the PC was off.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};

pub fn parse_hm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

fn at(date: chrono::NaiveDate, t: NaiveTime) -> Option<DateTime<Local>> {
    Local.from_local_datetime(&date.and_time(t)).earliest()
}

/// Most recent daily slot at or before `now`.
pub fn last_daily_slot(now: DateTime<Local>, t: NaiveTime) -> Option<DateTime<Local>> {
    let today = at(now.date_naive(), t)?;
    if today <= now { Some(today) } else { at(now.date_naive() - Duration::days(1), t) }
}

/// Most recent weekly slot (weekday 0 = Monday) at or before `now`.
pub fn last_weekly_slot(now: DateTime<Local>, weekday: u8, t: NaiveTime) -> Option<DateTime<Local>> {
    let back = (i64::from(now.weekday().num_days_from_monday()) - i64::from(weekday % 7)).rem_euclid(7);
    let slot = at(now.date_naive() - Duration::days(back), t)?;
    if slot <= now { Some(slot) } else { at(slot.date_naive() - Duration::days(7), t) }
}

/// Due if the latest slot has not run yet and is recent enough to still be useful.
pub fn is_due(slot: Option<DateTime<Local>>, last_run: Option<DateTime<Local>>, now: DateTime<Local>, catch_up: Duration) -> bool {
    match slot {
        Some(slot) => now - slot <= catch_up && last_run.is_none_or(|l| l < slot),
        None => false,
    }
}

/// Quiet hours, which may wrap past midnight (22:30 → 08:00).
pub fn in_quiet_hours(now: DateTime<Local>, from: &str, to: &str) -> bool {
    let (Some(f), Some(t)) = (parse_hm(from), parse_hm(to)) else { return false };
    let n = now.time();
    if f <= t { n >= f && n < t } else { n >= f || n < t }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn daily_slots_and_catch_up() {
        let t = parse_hm("21:00").unwrap();
        let now = local(2026, 11, 27, 22, 0);
        let slot = last_daily_slot(now, t);
        assert_eq!(slot, Some(local(2026, 11, 27, 21, 0)));
        assert!(is_due(slot, None, now, Duration::hours(12)));
        assert!(!is_due(slot, Some(local(2026, 11, 27, 21, 5)), now, Duration::hours(12)));
        let morning = local(2026, 11, 28, 8, 0);
        assert_eq!(last_daily_slot(morning, t), Some(local(2026, 11, 27, 21, 0)));
        // PC was off the whole next day: too late, skip.
        assert!(!is_due(last_daily_slot(local(2026, 11, 28, 20, 0), t), None, local(2026, 11, 28, 20, 0), Duration::hours(12)));
    }

    #[test]
    fn weekly_slots() {
        let t = parse_hm("20:00").unwrap();
        // 2026-11-29 is a Sunday.
        let now = local(2026, 11, 30, 9, 0);
        assert_eq!(last_weekly_slot(now, 6, t), Some(local(2026, 11, 29, 20, 0)));
        assert_eq!(last_weekly_slot(local(2026, 11, 29, 19, 0), 6, t), Some(local(2026, 11, 22, 20, 0)));
    }

    #[test]
    fn quiet_hours_wrap() {
        assert!(in_quiet_hours(local(2026, 1, 1, 23, 0), "22:30", "08:00"));
        assert!(in_quiet_hours(local(2026, 1, 1, 7, 59), "22:30", "08:00"));
        assert!(!in_quiet_hours(local(2026, 1, 1, 12, 0), "22:30", "08:00"));
        assert!(in_quiet_hours(local(2026, 1, 1, 13, 0), "12:00", "14:00"));
    }
}
