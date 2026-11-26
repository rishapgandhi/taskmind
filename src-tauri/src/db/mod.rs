//! SQLite storage: connection, migrations and small shared queries.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

mod activities;
pub(crate) mod meetings;
mod notes;
mod reports;

pub use activities::{ActivityRow, NewActivity, ProjectRow, RULE_KINDS, RuleRow};
pub use meetings::{MeetingRow, SearchHit, SegmentRow};
pub use notes::{DayActionItem, StoredNotes};

use rusqlite::{Connection, OptionalExtension, params};

/// Ordered list of migrations. Index + 1 is the schema version stored in `PRAGMA user_version`.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_transcription.sql"),
    include_str!("../../migrations/0003_timesheet_names.sql"),
    include_str!("../../migrations/0004_notes_error.sql"),
];

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not create data folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid stored value: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database lock was poisoned by a crashed thread")]
    Poisoned,
    #[error("database schema version {found} is newer than this app supports ({supported})")]
    TooNew { found: usize, supported: usize },
}

pub struct Db {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        Self::init(conn, Some(path.to_path_buf()))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(mut conn: Connection, path: Option<PathBuf>) -> Result<Self, DbError> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    /// Consistent copy of the whole database (safe while the app is running).
    pub fn vacuum_into(&self, target: &Path) -> Result<(), DbError> {
        self.lock()?
            .execute("VACUUM INTO ?1", params![target.display().to_string()])?;
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.conn.lock().map_err(|_| DbError::Poisoned)
    }

    pub fn schema_version(&self) -> Result<usize, DbError> {
        user_version(&*self.lock()?)
    }

    pub fn count(&self, table: Table) -> Result<i64, DbError> {
        let sql = format!("SELECT COUNT(*) FROM {}", table.name());
        Ok(self.lock()?.query_row(&sql, [], |row| row.get(0))?)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, DbError> {
        let raw: Option<String> = self
            .lock()?
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|s| serde_json::from_str(&s).map_err(DbError::from))
            .transpose()
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), DbError> {
        let raw = serde_json::to_string(value)?;
        self.lock()?.execute(
            "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![key, raw],
        )?;
        Ok(())
    }
}

/// Tables that may be counted. A fixed list keeps table names out of user input.
#[derive(Clone, Copy)]
pub enum Table {
    Meetings,
    Activities,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Table::Meetings => "meetings",
            Table::Activities => "activities",
        }
    }
}

fn user_version(conn: &Connection) -> Result<usize, DbError> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(usize::try_from(v).unwrap_or(0))
}

fn migrate(conn: &mut Connection) -> Result<(), DbError> {
    let current = user_version(conn)?;
    if current > MIGRATIONS.len() {
        return Err(DbError::TooNew {
            found: current,
            supported: MIGRATIONS.len(),
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = index + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version as i64)?;
        tx.commit()?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn migrates_to_latest_version() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len());
        assert_eq!(db.count(Table::Meetings).unwrap(), 0);
        assert_eq!(db.count(Table::Activities).unwrap(), 0);
    }

    #[test]
    fn reopening_does_not_rerun_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("t.db");
        Db::open(&path)
            .unwrap()
            .set_setting("a", &json!(1))
            .unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(db.get_setting("a").unwrap(), Some(json!(1)));
    }

    #[test]
    fn settings_round_trip_and_overwrite() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("missing").unwrap(), None);
        db.set_setting("k", &json!({"on": true})).unwrap();
        db.set_setting("k", &json!(false)).unwrap();
        assert_eq!(db.get_setting("k").unwrap(), Some(json!(false)));
    }

    #[test]
    fn transcript_search_finds_inserted_text() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO meetings (started_at, created_at) VALUES ('2026-11-27T10:00:00Z', '2026-11-27T10:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transcript_segments (meeting_id, start_ms, end_ms, text) VALUES (1, 0, 1000, 'ship the invoice module on Friday')",
            [],
        )
        .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM segments_fts WHERE segments_fts MATCH 'invoice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }
}
