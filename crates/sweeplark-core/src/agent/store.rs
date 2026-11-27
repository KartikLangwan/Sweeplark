//! Local storage for the background agent: PC samples, inbox messages, pending approvals,
//! and small bits of state (last run times, cooldowns). Old rows are pruned automatically
//! so the file stays small (a few MB at most).

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::Result;

/// One light measurement, taken every few minutes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub at: DateTime<Utc>,
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub ram_used: u64,
    pub system_free: u64,
    pub system_total: u64,
    /// Heaviest non-system app by RAM, with its bytes.
    pub top_ram_app: Option<(String, u64)>,
    /// Busiest non-system app by CPU, with its percent.
    pub top_cpu_app: Option<(String, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Alert,
    Daily,
    Weekly,
    Cleanup,
    Approval,
    Tip,
}

impl MessageKind {
    fn as_str(self) -> &'static str {
        match self {
            MessageKind::Alert => "alert",
            MessageKind::Daily => "daily",
            MessageKind::Weekly => "weekly",
            MessageKind::Cleanup => "cleanup",
            MessageKind::Approval => "approval",
            MessageKind::Tip => "tip",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "alert" => MessageKind::Alert,
            "daily" => MessageKind::Daily,
            "weekly" => MessageKind::Weekly,
            "cleanup" => MessageKind::Cleanup,
            "approval" => MessageKind::Approval,
            _ => MessageKind::Tip,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub kind: MessageKind,
    pub title: String,
    /// The app's own text (always correct numbers).
    pub body: String,
    /// Optional friendlier text written by the local AI.
    pub ai_body: Option<String>,
    pub read: bool,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Declined,
    Expired,
}

impl ApprovalStatus {
    fn as_str(self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Declined => "declined",
            ApprovalStatus::Expired => "expired",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "approved" => ApprovalStatus::Approved,
            "declined" => ApprovalStatus::Declined,
            "expired" => ApprovalStatus::Expired,
            _ => ApprovalStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub status: ApprovalStatus,
    pub title: String,
    pub detail: serde_json::Value,
}

pub struct AgentStore {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS samples (
    at INTEGER NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS samples_at ON samples(at);
CREATE TABLE IF NOT EXISTS inbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    ai_body TEXT,
    read INTEGER NOT NULL DEFAULT 0,
    data TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS approvals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

fn ts(t: DateTime<Utc>) -> i64 {
    t.timestamp()
}

fn from_ts(s: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(s, 0).unwrap_or_default()
}

impl AgentStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    // ---- samples ----

    pub fn add_sample(&self, s: &Sample) -> Result<()> {
        self.conn.execute("INSERT INTO samples (at, data) VALUES (?1, ?2)", params![ts(s.at), serde_json::to_string(s)?])?;
        Ok(())
    }

    pub fn samples_between(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<Sample>> {
        let mut stmt = self.conn.prepare("SELECT data FROM samples WHERE at >= ?1 AND at < ?2 ORDER BY at")?;
        let rows = stmt.query_map(params![ts(from), ts(to)], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    }

    // ---- inbox ----

    pub fn add_message(
        &self,
        kind: MessageKind,
        title: &str,
        body: &str,
        ai_body: Option<&str>,
        data: &serde_json::Value,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO inbox (at, kind, title, body, ai_body, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![ts(Utc::now()), kind.as_str(), title, body, ai_body, serde_json::to_string(data)?],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn messages(&self, limit: usize) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, at, kind, title, body, ai_body, read, data FROM inbox ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(Message {
                id: r.get(0)?,
                at: from_ts(r.get(1)?),
                kind: MessageKind::parse(&r.get::<_, String>(2)?),
                title: r.get(3)?,
                body: r.get(4)?,
                ai_body: r.get(5)?,
                read: r.get::<_, i64>(6)? != 0,
                data: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn mark_read(&self, id: Option<i64>) -> Result<()> {
        match id {
            Some(id) => self.conn.execute("UPDATE inbox SET read = 1 WHERE id = ?1", [id])?,
            None => self.conn.execute("UPDATE inbox SET read = 1", [])?,
        };
        Ok(())
    }

    pub fn unread_count(&self) -> Result<u64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM inbox WHERE read = 0", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Notifications sent since `since` (for the daily limit).
    pub fn notified_since(&self, since: DateTime<Utc>) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM inbox WHERE at >= ?1 AND kind IN ('alert', 'approval', 'cleanup')",
            [ts(since)],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    // ---- approvals ----

    pub fn add_approval(&self, title: &str, detail: &serde_json::Value) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO approvals (at, status, title, detail) VALUES (?1, 'pending', ?2, ?3)",
            params![ts(Utc::now()), title, serde_json::to_string(detail)?],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn approval_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Approval> {
        Ok(Approval {
            id: r.get(0)?,
            at: from_ts(r.get(1)?),
            status: ApprovalStatus::parse(&r.get::<_, String>(2)?),
            title: r.get(3)?,
            detail: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
        })
    }

    pub fn pending_approvals(&self) -> Result<Vec<Approval>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, at, status, title, detail FROM approvals WHERE status = 'pending' ORDER BY id DESC")?;
        let rows = stmt.query_map([], Self::approval_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn approval(&self, id: i64) -> Result<Option<Approval>> {
        Ok(self
            .conn
            .query_row("SELECT id, at, status, title, detail FROM approvals WHERE id = ?1", [id], Self::approval_row)
            .optional()?)
    }

    pub fn set_approval_status(&self, id: i64, status: ApprovalStatus) -> Result<()> {
        self.conn.execute("UPDATE approvals SET status = ?2 WHERE id = ?1", params![id, status.as_str()])?;
        Ok(())
    }

    /// Pending approvals older than `max_age_hours` expire (nothing is deleted for them).
    pub fn expire_approvals(&self, now: DateTime<Utc>, max_age_hours: i64) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE approvals SET status = 'expired' WHERE status = 'pending' AND at < ?1",
            [ts(now) - max_age_hours * 3600],
        )?)
    }

    // ---- state ----

    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM state WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_time(&self, key: &str) -> Result<Option<DateTime<Utc>>> {
        Ok(self.get_state(key)?.and_then(|v| DateTime::parse_from_rfc3339(&v).ok()).map(|d| d.with_timezone(&Utc)))
    }

    pub fn set_time(&self, key: &str, t: DateTime<Utc>) -> Result<()> {
        self.set_state(key, &t.to_rfc3339())
    }

    /// Keep the file small: samples for 15 days, messages and approvals for 60 days.
    pub fn prune(&self, now: DateTime<Utc>) -> Result<()> {
        let day = 86_400;
        self.conn.execute("DELETE FROM samples WHERE at < ?1", [ts(now) - 15 * day])?;
        self.conn.execute("DELETE FROM inbox WHERE at < ?1", [ts(now) - 60 * day])?;
        self.conn.execute("DELETE FROM approvals WHERE at < ?1 AND status != 'pending'", [ts(now) - 60 * day])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: DateTime<Utc>, ram: f32) -> Sample {
        Sample {
            at,
            cpu_percent: 10.0,
            ram_percent: ram,
            ram_used: 1,
            system_free: 50,
            system_total: 100,
            top_ram_app: Some(("chrome".into(), 2 << 30)),
            top_cpu_app: None,
        }
    }

    #[test]
    fn samples_messages_approvals_state() {
        let s = AgentStore::open_in_memory().unwrap();
        let now = Utc::now();
        s.add_sample(&sample(now - chrono::Duration::days(20), 50.0)).unwrap();
        s.add_sample(&sample(now, 80.0)).unwrap();
        assert_eq!(s.samples_between(now - chrono::Duration::days(30), now + chrono::Duration::seconds(1)).unwrap().len(), 2);
        s.prune(now).unwrap();
        assert_eq!(s.samples_between(now - chrono::Duration::days(30), now + chrono::Duration::seconds(1)).unwrap().len(), 1);

        let id = s.add_message(MessageKind::Daily, "Aaj ki report", "body", Some("ai"), &serde_json::json!({})).unwrap();
        assert_eq!(s.unread_count().unwrap(), 1);
        s.mark_read(Some(id)).unwrap();
        assert_eq!(s.unread_count().unwrap(), 0);
        assert_eq!(s.messages(10).unwrap()[0].ai_body.as_deref(), Some("ai"));

        let a = s.add_approval("Temp files", &serde_json::json!({"bytes": 5})).unwrap();
        assert_eq!(s.pending_approvals().unwrap().len(), 1);
        s.set_approval_status(a, ApprovalStatus::Approved).unwrap();
        assert!(s.pending_approvals().unwrap().is_empty());

        s.set_time("last_daily", now).unwrap();
        assert_eq!(s.get_time("last_daily").unwrap().unwrap().timestamp(), now.timestamp());
    }
}
