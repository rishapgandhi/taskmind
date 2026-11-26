//! Meeting rows.

use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;

use super::{Db, DbError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRow {
    pub id: i64,
    pub title: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_s: Option<i64>,
    pub audio_path: Option<String>,
    pub audio_bytes: Option<i64>,
    pub status: String,
    pub language: Option<String>,
    pub error: Option<String>,
    pub has_notes: bool,
    pub notes_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentRow {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub meeting_id: i64,
    pub title: Option<String>,
    pub started_at: String,
    pub start_ms: i64,
    pub speaker: String,
    pub snippet: String,
}

impl MeetingRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            title: row.get(1)?,
            started_at: row.get(2)?,
            ended_at: row.get(3)?,
            duration_s: row.get(4)?,
            audio_path: row.get(5)?,
            audio_bytes: row.get(6)?,
            status: row.get(7)?,
            language: row.get(8)?,
            error: row.get(9)?,
            has_notes: row.get(10)?,
            notes_error: row.get(11)?,
        })
    }
}

const COLUMNS: &str =
    "id, title, started_at, ended_at, duration_s, audio_path, audio_bytes, status, language, error,
     EXISTS(SELECT 1 FROM meeting_notes n WHERE n.meeting_id = meetings.id), notes_error";

impl Db {
    pub fn create_meeting(&self, started_at: &str, audio_path: &str) -> Result<i64, DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO meetings (started_at, audio_path, status, created_at)
             VALUES (?1, ?2, 'recording', ?1)",
            params![started_at, audio_path],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Mark a finished recording as waiting for transcription (M2 picks up `queued` rows).
    pub fn finish_meeting(
        &self,
        id: i64,
        ended_at: &str,
        duration_s: i64,
        audio_bytes: i64,
    ) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE meetings SET ended_at = ?2, duration_s = ?3, audio_bytes = ?4, status = 'queued'
             WHERE id = ?1",
            params![id, ended_at, duration_s, audio_bytes],
        )?;
        Ok(())
    }

    pub fn fail_meeting(&self, id: i64, ended_at: &str) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE meetings SET ended_at = ?2, status = 'failed' WHERE id = ?1",
            params![id, ended_at],
        )?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: i64) -> Result<(), DbError> {
        self.lock()?
            .execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Rows left in `recording` after a crash: keep the audio (written page by page) and queue it.
    pub fn recover_interrupted(&self) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let rows: Vec<(i64, Option<String>)> = conn
            .prepare("SELECT id, audio_path FROM meetings WHERE status = 'recording'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (id, path) in &rows {
            let bytes = path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len() as i64);
            conn.execute(
                "UPDATE meetings SET status = 'queued', audio_bytes = ?2,
                 title = COALESCE(title, 'Recovered recording') WHERE id = ?1",
                params![id, bytes],
            )?;
        }
        Ok(rows.len())
    }

    pub fn list_meetings(&self, limit: u32) -> Result<Vec<MeetingRow>, DbError> {
        let conn = self.lock()?;
        let sql = format!("SELECT {COLUMNS} FROM meetings ORDER BY started_at DESC LIMIT ?1");
        let rows = conn
            .prepare(&sql)?
            .query_map(params![limit], MeetingRow::from_row)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn get_meeting(&self, id: i64) -> Result<Option<MeetingRow>, DbError> {
        let sql = format!("SELECT {COLUMNS} FROM meetings WHERE id = ?1");
        Ok(self
            .lock()?
            .query_row(&sql, params![id], MeetingRow::from_row)
            .optional()?)
    }
}

impl Db {
    /// Oldest meeting waiting for a transcript.
    pub fn next_queued(&self) -> Result<Option<MeetingRow>, DbError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM meetings WHERE status = 'queued' ORDER BY started_at LIMIT 1"
        );
        Ok(self
            .lock()?
            .query_row(&sql, [], MeetingRow::from_row)
            .optional()?)
    }

    pub fn set_meeting_status(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE meetings SET status = ?2, error = ?3 WHERE id = ?1",
            params![id, status, error],
        )?;
        Ok(())
    }

    /// Jobs cut off by a quit or crash go back in the queue.
    pub fn requeue_interrupted_transcriptions(&self) -> Result<usize, DbError> {
        Ok(self.lock()?.execute(
            "UPDATE meetings SET status = 'queued' WHERE status = 'transcribing'",
            [],
        )?)
    }

    pub fn save_transcript(
        &self,
        id: i64,
        segments: &[SegmentRow],
        language: Option<&str>,
        model: &str,
    ) -> Result<(), DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM transcript_segments WHERE meeting_id = ?1",
            params![id],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO transcript_segments (meeting_id, start_ms, end_ms, speaker, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for s in segments {
                insert.execute(params![id, s.start_ms, s.end_ms, s.speaker, s.text])?;
            }
        }
        tx.execute(
            "UPDATE meetings SET status = 'done', error = NULL, language = ?2, transcript_model = ?3
             WHERE id = ?1",
            params![id, language, model],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn segments(&self, id: i64) -> Result<Vec<SegmentRow>, DbError> {
        let conn = self.lock()?;
        let rows = conn
            .prepare(
                "SELECT start_ms, end_ms, speaker, text FROM transcript_segments
                 WHERE meeting_id = ?1 ORDER BY start_ms",
            )?
            .query_map(params![id], |r| {
                Ok(SegmentRow {
                    start_ms: r.get(0)?,
                    end_ms: r.get(1)?,
                    speaker: r.get(2)?,
                    text: r.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn rename_meeting(&self, id: i64, title: Option<&str>) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE meetings SET title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        Ok(())
    }

    /// Full-text search across transcripts. User text is quoted term by term, so it can never
    /// be read as FTS syntax. The last term matches as a prefix ("invo" finds "invoice").
    pub fn search_transcripts(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, DbError> {
        let Some(fts) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let conn = self.lock()?;
        let rows = conn
            .prepare(
                "SELECT m.id, m.title, m.started_at, s.start_ms, s.speaker,
                        snippet(segments_fts, 0, '[', ']', '…', 12)
                 FROM segments_fts
                 JOIN transcript_segments s ON s.id = segments_fts.rowid
                 JOIN meetings m ON m.id = s.meeting_id
                 WHERE segments_fts MATCH ?1
                 ORDER BY m.started_at DESC, s.start_ms
                 LIMIT ?2",
            )?
            .query_map(params![fts, limit], |r| {
                Ok(SearchHit {
                    meeting_id: r.get(0)?,
                    title: r.get(1)?,
                    started_at: r.get(2)?,
                    start_ms: r.get(3)?,
                    speaker: r.get(4)?,
                    snippet: r.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Db {
    /// Meetings with audio that started before `before` and are no longer being processed.
    pub fn meetings_with_audio_before(&self, before: &str) -> Result<Vec<(i64, String)>, DbError> {
        let conn = self.lock()?;
        let rows = conn
            .prepare(
                "SELECT id, audio_path FROM meetings WHERE audio_path IS NOT NULL AND audio_path != ''
                 AND started_at < ?1 AND status IN ('done', 'failed')",
            )?
            .query_map(params![before], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn set_notes_error(&self, id: i64, error: Option<&str>) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE meetings SET notes_error = ?2 WHERE id = ?1",
            params![id, error],
        )?;
        Ok(())
    }

    pub fn clear_audio(&self, id: i64) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE meetings SET audio_path = NULL, audio_bytes = NULL WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

pub(crate) fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\""))
        .collect();
    if terms.is_empty() {
        return None;
    }
    let mut q = terms.join(" ");
    q.push('*');
    Some(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .create_meeting("2026-11-27T10:00:00Z", "/tmp/a.ogg")
            .unwrap();
        assert_eq!(db.get_meeting(id).unwrap().unwrap().status, "recording");
        db.finish_meeting(id, "2026-11-27T10:30:00Z", 1800, 12345)
            .unwrap();
        let m = db.get_meeting(id).unwrap().unwrap();
        assert_eq!(m.status, "queued");
        assert_eq!(m.duration_s, Some(1800));
        assert_eq!(db.list_meetings(10).unwrap().len(), 1);
        db.delete_meeting(id).unwrap();
        assert!(db.get_meeting(id).unwrap().is_none());
    }

    #[test]
    fn interrupted_recordings_are_recovered() {
        let db = Db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ogg");
        std::fs::write(&path, [0u8; 42]).unwrap();
        let id = db
            .create_meeting("2026-11-27T10:00:00Z", &path.display().to_string())
            .unwrap();
        assert_eq!(db.recover_interrupted().unwrap(), 1);
        let m = db.get_meeting(id).unwrap().unwrap();
        assert_eq!(m.status, "queued");
        assert_eq!(m.audio_bytes, Some(42));
        assert_eq!(m.title.as_deref(), Some("Recovered recording"));
        assert_eq!(db.recover_interrupted().unwrap(), 0);
    }

    #[test]
    fn transcript_save_search_and_queue() {
        let db = Db::open_in_memory().unwrap();
        let a = db.create_meeting("2026-11-27T09:00:00Z", "a").unwrap();
        let b = db.create_meeting("2026-11-27T10:00:00Z", "b").unwrap();
        db.finish_meeting(a, "x", 60, 1).unwrap();
        db.finish_meeting(b, "x", 60, 1).unwrap();
        assert_eq!(db.next_queued().unwrap().unwrap().id, a);

        let segs = vec![
            SegmentRow {
                start_ms: 0,
                end_ms: 2000,
                speaker: "you".into(),
                text: "Let's ship the invoice module".into(),
            },
            SegmentRow {
                start_ms: 2000,
                end_ms: 4000,
                speaker: "others".into(),
                text: "Friday works \"for\" us".into(),
            },
        ];
        db.save_transcript(a, &segs, Some("en"), "base").unwrap();
        assert_eq!(db.get_meeting(a).unwrap().unwrap().status, "done");
        assert_eq!(db.segments(a).unwrap().len(), 2);
        assert_eq!(db.next_queued().unwrap().unwrap().id, b);

        let hits = db.search_transcripts("invo", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start_ms, 0);
        assert!(hits[0].snippet.contains("[invoice]"));
        // Odd user input must not break the query.
        assert!(db.search_transcripts("\"for\" AND (", 10).is_ok());
        assert!(db.search_transcripts("   ", 10).unwrap().is_empty());

        // Saving again replaces, not duplicates.
        db.save_transcript(a, &segs[..1], Some("en"), "base")
            .unwrap();
        assert_eq!(db.segments(a).unwrap().len(), 1);
        assert_eq!(db.search_transcripts("friday", 10).unwrap().len(), 0);
    }

    #[test]
    fn list_is_newest_first() {
        let db = Db::open_in_memory().unwrap();
        db.create_meeting("2026-11-26T10:00:00Z", "a").unwrap();
        db.create_meeting("2026-11-27T10:00:00Z", "b").unwrap();
        let list = db.list_meetings(10).unwrap();
        assert_eq!(list[0].audio_path.as_deref(), Some("b"));
    }
}
