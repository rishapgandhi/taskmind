//! Queries for reports and saved timesheets.

use rusqlite::params;

use super::meetings::MeetingRow;
use super::{Db, DbError};
use crate::timesheet::Entry;

impl Db {
    /// Meetings overlapping [from, to).
    pub fn meetings_between(&self, from: &str, to: &str) -> Result<Vec<MeetingRow>, DbError> {
        Ok(self
            .list_meetings(1000)?
            .into_iter()
            .filter(|m| {
                m.started_at.as_str() < to && m.ended_at.as_deref().is_none_or(|e| e > from)
            })
            .collect())
    }

    /// Saved timesheet for a local date: (status, entries), or None if never saved.
    pub fn saved_timesheet(&self, date: &str) -> Result<Option<(String, Vec<Entry>)>, DbError> {
        let conn = self.lock()?;
        let rows: Vec<(String, Entry)> = conn
            .prepare(
                "SELECT status, project_id, COALESCE(project_name, ''), start_at, end_at, minutes,
                        COALESCE(description, '')
                 FROM timesheet_entries WHERE date = ?1 ORDER BY start_at, id",
            )?
            .query_map(params![date], |r| {
                Ok((
                    r.get(0)?,
                    Entry {
                        project_id: r.get(1)?,
                        project_name: r.get(2)?,
                        start: r.get(3)?,
                        end: r.get(4)?,
                        minutes: r.get(5)?,
                        description: r.get(6)?,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?;
        if rows.is_empty() {
            return Ok(None);
        }
        let status = if rows.iter().all(|(s, _)| s == "approved") {
            "approved"
        } else {
            "draft"
        };
        Ok(Some((
            status.to_string(),
            rows.into_iter().map(|(_, e)| e).collect(),
        )))
    }

    pub fn save_timesheet(
        &self,
        date: &str,
        entries: &[Entry],
        approved: bool,
    ) -> Result<(), DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM timesheet_entries WHERE date = ?1",
            params![date],
        )?;
        let status = if approved { "approved" } else { "draft" };
        for e in entries {
            tx.execute(
                "INSERT INTO timesheet_entries
                 (date, project_id, project_name, start_at, end_at, minutes, description, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    date,
                    e.project_id,
                    e.project_name,
                    e.start,
                    e.end,
                    e.minutes,
                    e.description,
                    status
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_timesheet(&self, date: &str) -> Result<(), DbError> {
        self.lock()?.execute(
            "DELETE FROM timesheet_entries WHERE date = ?1",
            params![date],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timesheet_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.saved_timesheet("2026-11-27").unwrap().is_none());
        let e = Entry {
            project_id: None,
            project_name: "Meetings".into(),
            start: "10:00".into(),
            end: "10:30".into(),
            minutes: 30,
            description: "Standup".into(),
        };
        db.save_timesheet("2026-11-27", std::slice::from_ref(&e), false)
            .unwrap();
        let (status, rows) = db.saved_timesheet("2026-11-27").unwrap().unwrap();
        assert_eq!(status, "draft");
        assert_eq!(rows, vec![e.clone()]);
        db.save_timesheet("2026-11-27", &[e], true).unwrap();
        assert_eq!(
            db.saved_timesheet("2026-11-27").unwrap().unwrap().0,
            "approved"
        );
        db.clear_timesheet("2026-11-27").unwrap();
        assert!(db.saved_timesheet("2026-11-27").unwrap().is_none());
    }

    #[test]
    fn meetings_overlap_filter() {
        let db = Db::open_in_memory().unwrap();
        let a = db.create_meeting("2026-11-27T10:00:00Z", "a").unwrap();
        db.finish_meeting(a, "2026-11-27T11:00:00Z", 3600, 1)
            .unwrap();
        db.create_meeting("2026-11-28T10:00:00Z", "b").unwrap();
        let m = db
            .meetings_between("2026-11-27T00:00:00Z", "2026-11-28T00:00:00Z")
            .unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, a);
    }
}
