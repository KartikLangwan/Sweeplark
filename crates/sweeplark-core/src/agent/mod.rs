//! Background agent: runs inside the tray app every minute. Cheap work only (a light
//! sample every few minutes); the AI model is loaded just for a summary, an alert text or
//! a file review, and released at the end of the tick.

pub mod autoclean;
pub mod schedule;
pub mod store;
pub mod summary;
pub mod triggers;

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use serde::Serialize;

use self::store::{AgentStore, MessageKind, Sample};
use crate::ai::review::{ReviewItem, Verdict};
use crate::cleaner::{RuleSet, ScanContext};
use crate::health::{self, HealthSnapshot};
use crate::ledger::Ledger;
use crate::settings::{Frequency, Settings};
use crate::startup::StartupItem;
use crate::Result;

/// Things only the app shell can do (notifications, the AI engine, reading the PC).
pub trait AgentHost {
    fn notify(&mut self, title: &str, body: &str, sound: bool);
    /// Friendly text from the local model, or `None` when AI is off / unavailable.
    fn narrate(&mut self, kind: &str, points: &[String], facts: &str) -> Option<String>;
    /// AI second opinion on unclear files, or `None` when AI is off.
    fn review(&mut self, items: &[ReviewItem]) -> Option<Vec<Verdict>>;
    /// Unload the model if this tick loaded it.
    fn release_model(&mut self);
    fn idle_seconds(&self) -> u64;
    fn health(&mut self, include_slow: bool) -> HealthSnapshot {
        health::snapshot(include_slow)
    }
    fn startup_items(&mut self) -> Vec<StartupItem>;
    fn scan_context(&mut self, settings: &Settings) -> ScanContext {
        ScanContext::system(settings)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TickReport {
    pub sampled: bool,
    pub alerts: Vec<String>,
    pub auto_clean: bool,
    pub approval_id: Option<i64>,
    pub daily: bool,
    pub weekly: bool,
}

pub fn sample_from(h: &HealthSnapshot) -> Sample {
    let sys = h.system_disk();
    Sample {
        at: h.taken_at,
        cpu_percent: h.cpu_percent,
        ram_percent: h.memory_percent() as f32,
        ram_used: h.memory_used,
        system_free: sys.map(|d| d.free_bytes).unwrap_or(0),
        system_total: sys.map(|d| d.total_bytes).unwrap_or(0),
        top_ram_app: h.top_memory.iter().find(|a| !a.system).map(|a| (a.name.clone(), a.memory_bytes)),
        top_cpu_app: h.top_cpu.iter().find(|a| !a.system).map(|a| (a.name.clone(), a.cpu_percent)),
    }
}

pub struct Agent<'a> {
    pub store: &'a AgentStore,
    pub ledger: &'a Ledger,
    pub rules: &'a RuleSet,
}

fn local(t: DateTime<Utc>) -> DateTime<Local> {
    t.with_timezone(&Local)
}

fn start_of_local_day(now: DateTime<Local>) -> DateTime<Utc> {
    Local
        .from_local_datetime(&now.date_naive().and_hms_opt(0, 0, 0).expect("midnight"))
        .earliest()
        .unwrap_or(now)
        .with_timezone(&Utc)
}

impl Agent<'_> {
    #[allow(clippy::too_many_arguments)]
    fn deliver(
        &self,
        s: &Settings,
        host: &mut dyn AgentHost,
        now: DateTime<Utc>,
        kind: MessageKind,
        title: &str,
        body: &str,
        ai_body: Option<&str>,
        data: serde_json::Value,
    ) -> Result<i64> {
        let n = &s.agent.notify;
        // The daily limit counts pop-ups really shown, not messages kept quietly in the inbox.
        let day_key = format!("notified:{}", local(now).format("%Y-%m-%d"));
        let shown_today: u64 = self.store.get_state(&day_key)?.and_then(|v| v.parse().ok()).unwrap_or(0);
        let exempt = matches!(kind, MessageKind::Daily | MessageKind::Weekly | MessageKind::Approval);
        let under_limit = exempt || shown_today < u64::from(n.max_per_day);
        let id = self.store.add_message(kind, title, body, ai_body, &data)?;
        if n.enabled && under_limit && !schedule::in_quiet_hours(local(now), &n.quiet_from, &n.quiet_to) {
            let text = match kind {
                MessageKind::Daily | MessageKind::Weekly => ai_body.unwrap_or(body),
                _ => body,
            };
            host.notify(title, text, n.sound);
            if !exempt {
                self.store.set_state(&day_key, &(shown_today + 1).to_string())?;
            }
        }
        Ok(id)
    }

    fn cooled_down(&self, key: &str, hours: i64, now: DateTime<Utc>) -> Result<bool> {
        Ok(self.store.get_time(&format!("cooldown:{key}"))?.is_none_or(|t| now - t >= Duration::hours(hours)))
    }

    /// One pass. Call about once a minute.
    pub fn tick(&self, s: &Settings, now: DateTime<Utc>, host: &mut dyn AgentHost) -> Result<TickReport> {
        let mut rep = TickReport::default();
        if !s.agent.enabled {
            return Ok(rep);
        }
        let result = self.tick_inner(s, now, host, &mut rep);
        host.release_model();
        result.map(|()| rep)
    }

    fn tick_inner(&self, s: &Settings, now: DateTime<Utc>, host: &mut dyn AgentHost, rep: &mut TickReport) -> Result<()> {
        let a = &s.agent;
        // 1. Light sample.
        let every = Duration::minutes(i64::from(a.sample_minutes.max(1)));
        if self.store.get_time("last_sample")?.is_none_or(|t| now - t >= every) {
            let h = host.health(false);
            self.store.add_sample(&sample_from(&h))?;
            self.store.set_time("last_sample", now)?;
            rep.sampled = true;
        }
        // 2. Housekeeping once a day.
        if self.store.get_time("last_prune")?.is_none_or(|t| now - t >= Duration::hours(24)) {
            self.store.prune(now)?;
            self.store.set_time("last_prune", now)?;
        }
        // Cheap: an old approval must not block scheduled cleanups for up to a day longer.
        self.store.expire_approvals(now, 48)?;
        // 3. Smart alerts.
        if a.smart_alerts {
            let recent = self.store.samples_between(now - Duration::minutes(i64::from(a.sample_minutes) * 4), now + Duration::seconds(1))?;
            let slow = if self.store.get_time("last_security")?.is_none_or(|t| now - t >= Duration::hours(6)) {
                self.store.set_time("last_security", now)?;
                Some(host.health(true))
            } else {
                None
            };
            let items = host.startup_items();
            // Remember every entry, on or off: an app the user switches back on is not "new".
            let current: BTreeSet<String> = items.iter().map(|i| i.id.clone()).collect();
            let seen: Option<BTreeSet<String>> =
                self.store.get_state("startup_seen")?.and_then(|v| serde_json::from_str(&v).ok());
            let new_names: Vec<String> = match &seen {
                Some(seen) => items.iter().filter(|i| i.enabled && !seen.contains(&i.id)).map(|i| i.name.clone()).collect(),
                None => Vec::new(), // first run: remember, do not alert
            };
            // An empty list usually means the registry could not be read this time: keep the
            // old memory instead of making every app look new next minute.
            if !items.is_empty() {
                let mut all_seen = seen.unwrap_or_default();
                all_seen.extend(current);
                self.store.set_state("startup_seen", &serde_json::to_string(&all_seen)?)?;
            }

            for alert in triggers::evaluate(&recent, slow.as_ref(), &new_names, s.language) {
                if !self.cooled_down(&alert.key, alert.cooldown_hours, now)? {
                    continue;
                }
                self.store.set_time(&format!("cooldown:{}", alert.key), now)?;
                self.deliver(s, host, now, MessageKind::Alert, &alert.title, &alert.body, None, serde_json::json!({ "key": alert.key }))?;
                rep.alerts.push(alert.key);
            }
        }
        // 4. Automatic cleanup.
        let ac = &a.auto_clean;
        let slot = schedule::parse_hm(&ac.time).and_then(|t| match ac.frequency {
            Frequency::Off => None,
            Frequency::Daily => schedule::last_daily_slot(local(now), t),
            Frequency::Weekly => schedule::last_weekly_slot(local(now), ac.weekday, t),
        });
        let catch_up = if ac.frequency == Frequency::Weekly { Duration::days(3) } else { Duration::hours(12) };
        let last_clean = self.store.get_time("last_auto_clean")?.map(local);
        let scheduled = schedule::is_due(slot, last_clean, local(now), catch_up);
        let disk_low = ac.when_disk_low
            && self.store.samples_between(now - Duration::hours(1), now + Duration::seconds(1))?.last().is_some_and(|x| {
                x.system_total > 0 && (x.system_free as f64 / x.system_total as f64) < 0.10
            })
            && self.cooled_down("autoclean.disk", 24, now)?;
        let idle_ok = !ac.only_when_idle || host.idle_seconds() >= 300;
        let has_pending = !self.store.pending_approvals()?.is_empty();
        if (scheduled || disk_low) && idle_ok && !has_pending {
            self.store.set_time("last_auto_clean", now)?;
            if disk_low {
                self.store.set_time("cooldown:autoclean.disk", now)?;
            }
            let ctx = host.scan_context(s);
            let outcome = if s.ai.enabled && s.ai.review_files {
                let mut reviewer = |items: &[ReviewItem]| host.review(items);
                autoclean::run(self.rules, s, &ctx, self.ledger, Some(&mut reviewer))?
            } else {
                autoclean::run(self.rules, s, &ctx, self.ledger, None)?
            };
            rep.auto_clean = true;
            if let Some(res) = &outcome.cleaned {
                let (freed, n) = (crate::util::format_bytes(res.freed_bytes), res.deleted_files);
                let title = crate::tr!(s.language, "Freed {freed}", "{freed} jagah khaali ki");
                let body = crate::tr!(
                    s.language,
                    "Removed {n} junk files. Personal files were not touched.",
                    "{n} bekaar files hatayi. Personal files ko haath nahi lagaya."
                );
                self.deliver(s, host, now, MessageKind::Cleanup, &title, &body, None, serde_json::to_value(res)?)?;
            }
            if let Some((title, detail)) = &outcome.approval {
                let id = self.store.add_approval(title, &serde_json::to_value(detail)?)?;
                let n = detail.risky_files.len();
                let body = if n == 0 {
                    crate::tr!(
                        s.language,
                        "Take a look and say yes or no. Nothing is deleted without your approval.",
                        "Dekh kar haan ya na kahein. Aapki ijazat ke bina kuch delete nahi hoga."
                    )
                } else {
                    crate::tr!(
                        s.language,
                        "{n} files look like documents or photos; you can pick them one by one.",
                        "{n} files documents/photos jaisi lagti hain; unhe aap ek-ek karke chun sakte hain."
                    )
                };
                self.deliver(s, host, now, MessageKind::Approval, title, &body, None, serde_json::json!({ "approval_id": id }))?;
                rep.approval_id = Some(id);
            }
        }
        // 5. Daily / weekly summaries.
        let mut summaries = Vec::new();
        if let Some(t) = a.daily_summary.as_deref().and_then(schedule::parse_hm) {
            let slot = schedule::last_daily_slot(local(now), t);
            if let Some(due) = slot.filter(|_| schedule::is_due(slot, self.store.get_time("last_daily").ok().flatten().map(local), local(now), Duration::hours(12))) {
                // Report the day of the slot (a 21:00 report caught up after midnight covers
                // yesterday, not the first minutes of today).
                summaries.push((false, start_of_local_day(due), "last_daily"));
            }
        }
        if let Some(w) = &a.weekly_summary
            && let Some(t) = schedule::parse_hm(&w.time)
        {
            let slot = schedule::last_weekly_slot(local(now), w.weekday, t);
            if schedule::is_due(slot, self.store.get_time("last_weekly")?.map(local), local(now), Duration::days(3)) {
                summaries.push((true, now - Duration::days(7), "last_weekly"));
            }
        }
        for (weekly, from, key) in summaries {
            self.store.set_time(key, now)?;
            let samples = self.store.samples_between(from, now)?;
            let changes = self.ledger.between(from, now)?;
            let alerts = self
                .store
                .messages(500)?
                .iter()
                .filter(|m| m.kind == MessageKind::Alert && m.at >= from)
                .count() as u32;
            let sum = summary::build(from, now, &samples, &changes, alerts);
            let (title, body, points) = sum.render(weekly, s.language);
            let ai = if s.ai.enabled && s.ai.write_summaries {
                host.narrate(if weekly { "weekly summary" } else { "daily summary" }, &points, &body)
            } else {
                None
            };
            let kind = if weekly { MessageKind::Weekly } else { MessageKind::Daily };
            self.deliver(s, host, now, kind, &title, &body, ai.as_deref(), serde_json::to_value(&sum)?)?;
            if weekly {
                rep.weekly = true;
            } else {
                rep.daily = true;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::facts::test_support;

    #[derive(Default)]
    struct FakeHost {
        notes: Vec<(String, String)>,
        narrated: u32,
        released: u32,
        startup: Vec<StartupItem>,
    }

    impl AgentHost for FakeHost {
        fn notify(&mut self, title: &str, body: &str, _sound: bool) {
            self.notes.push((title.into(), body.into()));
        }
        fn narrate(&mut self, _kind: &str, _points: &[String], _facts: &str) -> Option<String> {
            self.narrated += 1;
            Some("AI ne likha".into())
        }
        fn review(&mut self, _items: &[ReviewItem]) -> Option<Vec<Verdict>> {
            None
        }
        fn release_model(&mut self) {
            self.released += 1;
        }
        fn idle_seconds(&self) -> u64 {
            0
        }
        fn health(&mut self, _slow: bool) -> HealthSnapshot {
            test_support::health()
        }
        fn startup_items(&mut self) -> Vec<StartupItem> {
            self.startup.clone()
        }
    }

    #[test]
    fn tick_samples_summarises_and_releases_model() {
        let store = AgentStore::open_in_memory().unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        let rules = RuleSet::default();
        let agent = Agent { store: &store, ledger: &ledger, rules: &rules };
        let mut s = Settings::default();
        let now = Utc::now();
        let hm = |t: DateTime<Utc>| local(t).format("%H:%M").to_string();
        s.agent.daily_summary = Some(hm(now - Duration::minutes(2)));
        s.agent.weekly_summary = None;
        s.agent.auto_clean.frequency = Frequency::Off;
        s.agent.notify.quiet_from = "00:00".into();
        s.agent.notify.quiet_to = "00:00".into();
        s.ai.enabled = true;
        let mut host = FakeHost::default();

        let r = agent.tick(&s, now, &mut host).unwrap();
        // A summary scheduled 2 minutes ago is due unless that crossed midnight.
        if local(now - Duration::minutes(2)).date_naive() == local(now).date_naive() {
            assert!(r.daily);
            assert_eq!(host.narrated, 1);
            assert!(host.notes.iter().any(|(t, b)| t == "Today's report" && b == "AI ne likha"));
        }
        assert!(r.sampled);
        assert_eq!(host.released, 1);

        // Next minute: no new sample yet, no second summary.
        let r2 = agent.tick(&s, now + Duration::minutes(1), &mut host).unwrap();
        assert!(!r2.sampled && !r2.daily);
        assert_eq!(store.messages(10).unwrap().iter().filter(|m| m.kind == MessageKind::Daily).count(), usize::from(r.daily));
    }

    #[test]
    fn sustained_ram_alert_once_with_cooldown() {
        let store = AgentStore::open_in_memory().unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        let rules = RuleSet::default();
        let agent = Agent { store: &store, ledger: &ledger, rules: &rules };
        let mut s = Settings::default();
        s.agent.daily_summary = None;
        s.agent.weekly_summary = None;
        s.agent.auto_clean.frequency = Frequency::Off;
        s.agent.notify.quiet_from = "00:00".into();
        s.agent.notify.quiet_to = "00:00".into();
        let mut host = FakeHost::default();
        let t0 = Utc::now();
        // test_support::health() reports RAM ~91%.
        for i in 0..4 {
            agent.tick(&s, t0 + Duration::minutes(5 * i), &mut host).unwrap();
        }
        let alerts: Vec<_> = store.messages(20).unwrap().into_iter().filter(|m| m.kind == MessageKind::Alert).collect();
        assert_eq!(alerts.len(), 1, "one alert, then cooldown");
        assert!(alerts[0].title.contains("RAM"));
    }
}
