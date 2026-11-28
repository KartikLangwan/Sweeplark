//! The change ledger ("har badlav ka hisaab"): every change Sweeplark makes is recorded here,
//! together with the steps needed to undo it when undo is possible.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::platform::registry::RegistryBackend;
use crate::revert::{self, RevertStep};
use crate::{Error, Result, Risk};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Applied,
    Partial,
    Failed,
    Reverted,
}

impl ChangeStatus {
    fn as_str(self) -> &'static str {
        match self {
            ChangeStatus::Applied => "applied",
            ChangeStatus::Partial => "partial",
            ChangeStatus::Failed => "failed",
            ChangeStatus::Reverted => "reverted",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "partial" => ChangeStatus::Partial,
            "failed" => ChangeStatus::Failed,
            "reverted" => ChangeStatus::Reverted,
            _ => ChangeStatus::Applied,
        }
    }
}

/// A change about to be recorded.
#[derive(Debug, Clone)]
pub struct NewChange {
    /// "clean", "startup", "optimize" ...
    pub category: String,
    pub title: String,
    pub risk: Risk,
    pub status: ChangeStatus,
    pub bytes_freed: u64,
    pub items: u64,
    pub details: serde_json::Value,
    /// Empty = cannot be undone (e.g. deleted cache files).
    pub revert: Vec<RevertStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeRecord {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub category: String,
    pub title: String,
    pub risk: Risk,
    pub status: ChangeStatus,
    pub bytes_freed: u64,
    pub items: u64,
    pub details: serde_json::Value,
    pub revert: Vec<RevertStep>,
    pub can_undo: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LedgerSummary {
    pub changes: u64,
    pub bytes_freed: u64,
    pub undoable: u64,
}

pub struct Ledger {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS changes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    at          TEXT NOT NULL,
    category    TEXT NOT NULL,
    title       TEXT NOT NULL,
    risk        TEXT NOT NULL,
    status      TEXT NOT NULL,
    bytes_freed INTEGER NOT NULL DEFAULT 0,
    items       INTEGER NOT NULL DEFAULT 0,
    details     TEXT NOT NULL DEFAULT '{}',
    revert      TEXT NOT NULL DEFAULT '[]',
    reverted_at TEXT
);
CREATE INDEX IF NOT EXISTS changes_at ON changes(at);
PRAGMA user_version = 1;
";

fn risk_str(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "safe",
        Risk::Ask => "ask",
        Risk::Expert => "expert",
    }
}

fn parse_risk(s: &str) -> Risk {
    match s {
        "safe" => Risk::Safe,
        "expert" => Risk::Expert,
        _ => Risk::Ask,
    }
}

impl Ledger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn record(&self, change: &NewChange) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO changes (at, category, title, risk, status, bytes_freed, items, details, revert)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                Utc::now().to_rfc3339(),
                change.category,
                change.title,
                risk_str(change.risk),
                change.status.as_str(),
                change.bytes_freed as i64,
                change.items as i64,
                serde_json::to_string(&change.details)?,
                serde_json::to_string(&change.revert)?,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeRecord> {
        let at: String = row.get(1)?;
        let details: String = row.get(8)?;
        let revert: String = row.get(9)?;
        let status = ChangeStatus::parse(&row.get::<_, String>(5)?);
        let revert: Vec<RevertStep> = serde_json::from_str(&revert).unwrap_or_default();
        Ok(ChangeRecord {
            id: row.get(0)?,
            at: DateTime::parse_from_rfc3339(&at).map(|d| d.with_timezone(&Utc)).unwrap_or_default(),
            category: row.get(2)?,
            title: row.get(3)?,
            risk: parse_risk(&row.get::<_, String>(4)?),
            can_undo: !revert.is_empty() && matches!(status, ChangeStatus::Applied | ChangeStatus::Partial),
            status,
            bytes_freed: row.get::<_, i64>(6)? as u64,
            items: row.get::<_, i64>(7)? as u64,
            details: serde_json::from_str(&details).unwrap_or(serde_json::Value::Null),
            revert,
        })
    }

    const COLS: &'static str = "id, at, category, title, risk, status, bytes_freed, items, details, revert";

    pub fn list(&self, limit: usize) -> Result<Vec<ChangeRecord>> {
        let mut stmt =
            self.conn.prepare(&format!("SELECT {} FROM changes ORDER BY id DESC LIMIT ?1", Self::COLS))?;
        let rows = stmt.query_map([limit as i64], Self::row_to_record)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Changes made in `[from, to)`.
    pub fn between(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<ChangeRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM changes WHERE at >= ?1 AND at < ?2 ORDER BY id",
            Self::COLS
        ))?;
        let rows = stmt.query_map(params![from.to_rfc3339(), to.to_rfc3339()], Self::row_to_record)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Most recent change of `category` whose details have `"tweak_id": id` and that can still be undone.
    pub fn latest_undoable(&self, category: &str, tweak_id: &str) -> Result<Option<ChangeRecord>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM changes WHERE category = ?1 AND json_extract(details, '$.tweak_id') = ?2
                     AND status IN ('applied', 'partial') ORDER BY id DESC LIMIT 1",
                    Self::COLS
                ),
                params![category, tweak_id],
                Self::row_to_record,
            )
            .optional()?)
    }

    /// Newest change of a category that can still be undone.
    pub fn latest_undoable_in(&self, category: &str) -> Result<Option<ChangeRecord>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM changes WHERE category = ?1 AND revert != '[]'
                     AND status IN ('applied', 'partial') ORDER BY id DESC LIMIT 1",
                    Self::COLS
                ),
                params![category],
                Self::row_to_record,
            )
            .optional()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<ChangeRecord>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {} FROM changes WHERE id = ?1", Self::COLS), [id], Self::row_to_record)
            .optional()?)
    }

    pub fn summary(&self) -> Result<LedgerSummary> {
        let (changes, bytes): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(bytes_freed), 0) FROM changes WHERE status != 'failed'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let undoable: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM changes WHERE revert != '[]' AND status IN ('applied', 'partial')",
            [],
            |r| r.get(0),
        )?;
        Ok(LedgerSummary { changes: changes as u64, bytes_freed: bytes as u64, undoable: undoable as u64 })
    }

    /// Undo a change by running its revert steps, then mark it reverted.
    pub fn undo(&self, id: i64, reg: &dyn RegistryBackend) -> Result<ChangeRecord> {
        let record = self.get(id)?.ok_or_else(|| Error::NotFound(format!("change #{id}")))?;
        if record.status == ChangeStatus::Reverted {
            return Err(Error::Other(format!("change #{id} was already undone")));
        }
        if record.revert.is_empty() {
            return Err(Error::Other(format!("change #{id} cannot be undone (deleted cache does not come back)")));
        }
        revert::apply_all(&record.revert, reg)?;
        self.conn.execute(
            "UPDATE changes SET status = 'reverted', reverted_at = ?2 WHERE id = ?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        self.get(id)?.ok_or_else(|| Error::NotFound(format!("change #{id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::registry::{Hive, MemRegistry, RegValue};

    fn change(revert: Vec<RevertStep>) -> NewChange {
        NewChange {
            category: "startup".into(),
            title: "Disable Spotify at startup".into(),
            risk: Risk::Ask,
            status: ChangeStatus::Applied,
            bytes_freed: 0,
            items: 1,
            details: serde_json::json!({"name": "Spotify"}),
            revert,
        }
    }

    #[test]
    fn record_list_summary() {
        let l = Ledger::open_in_memory().unwrap();
        let mut c = change(vec![]);
        c.category = "clean".into();
        c.bytes_freed = 1000;
        l.record(&c).unwrap();
        let rows = l.list(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].can_undo);
        let s = l.summary().unwrap();
        assert_eq!(s.bytes_freed, 1000);
        assert_eq!(s.undoable, 0);
    }

    #[test]
    fn undo_restores_registry() {
        let reg = MemRegistry::default();
        reg.write_value(Hive::CurrentUser, "K", "v", &RegValue::Dword(3)).unwrap();
        let l = Ledger::open_in_memory().unwrap();
        let id = l
            .record(&change(vec![RevertStep::RegistryValue {
                hive: Hive::CurrentUser,
                key: "K".into(),
                name: "v".into(),
                previous: None,
            }]))
            .unwrap();
        assert_eq!(l.summary().unwrap().undoable, 1);
        let rec = l.undo(id, &reg).unwrap();
        assert_eq!(rec.status, ChangeStatus::Reverted);
        assert!(!rec.can_undo);
        assert_eq!(reg.read_value(Hive::CurrentUser, "K", "v").unwrap(), None);
        assert!(l.undo(id, &reg).is_err());
    }

    #[test]
    fn persists_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/ledger.db");
        {
            let l = Ledger::open(&path).unwrap();
            l.record(&change(vec![])).unwrap();
        }
        let l = Ledger::open(&path).unwrap();
        assert_eq!(l.list(5).unwrap().len(), 1);
    }
}
