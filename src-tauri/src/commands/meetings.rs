//! Meeting history commands.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::audio;
use crate::db::{Db, MeetingRow, SearchHit, SegmentRow};
use crate::error::{CommandError, CommandResult};
use crate::transcribe::Transcriber;

#[tauri::command]
pub fn list_meetings(
    db: State<'_, Db>,
    limit: Option<u32>,
    date_str: Option<String>,
) -> CommandResult<Vec<MeetingRow>> {
    let Some(date_str) = date_str else {
        return Ok(db.list_meetings(limit.unwrap_or(200).min(1000))?);
    };
    // Meetings that started on this local calendar day, newest first.
    let day = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| CommandError::new("invalid", "date must be YYYY-MM-DD"))?;
    let (from, to) = crate::timesheet::day_bounds(day);
    let (from, to) = (from.to_rfc3339(), to.to_rfc3339());
    let mut rows: Vec<MeetingRow> = db
        .list_meetings(1000)?
        .into_iter()
        .filter(|m| {
            crate::timesheet::parse(&m.started_at)
                .is_some_and(|s| s.to_rfc3339() >= from && s.to_rfc3339() < to)
        })
        .collect();
    rows.truncate(limit.unwrap_or(1000) as usize);
    Ok(rows)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetail {
    meeting: MeetingRow,
    segments: Vec<SegmentRow>,
}

fn find(db: &Db, id: i64) -> CommandResult<MeetingRow> {
    db.get_meeting(id)?
        .ok_or_else(|| CommandError::new("not_found", "meeting not found"))
}

#[tauri::command]
pub fn get_meeting(db: State<'_, Db>, id: i64) -> CommandResult<MeetingDetail> {
    Ok(MeetingDetail {
        meeting: find(&db, id)?,
        segments: db.segments(id)?,
    })
}

#[tauri::command]
pub fn rename_meeting(db: State<'_, Db>, id: i64, title: String) -> CommandResult<()> {
    let title = title.trim();
    if title.chars().count() > 200 {
        return Err(CommandError::new("invalid", "title is too long"));
    }
    find(&db, id)?;
    Ok(db.rename_meeting(id, (!title.is_empty()).then_some(title))?)
}

/// Delete a meeting with its audio, tracks and transcript.
#[tauri::command]
pub fn delete_meeting(app: AppHandle, db: State<'_, Db>, id: i64) -> CommandResult<()> {
    let meeting = find(&db, id)?;
    match meeting.status.as_str() {
        "recording" => {
            return Err(CommandError::new(
                "busy",
                "stop the recording before deleting it",
            ));
        }
        "transcribing" => {
            if let Some(t) = app.try_state::<Transcriber>() {
                t.abort_current();
            }
        }
        _ => {}
    }
    db.delete_meeting(id)?;
    if let Some(path) = meeting.audio_path.as_deref().map(std::path::Path::new) {
        let (mic, sys) = audio::track_paths(path);
        for p in [path.to_path_buf(), mic, sys] {
            if let Err(err) = std::fs::remove_file(&p)
                && err.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%err, path = %p.display(), "could not delete file");
            }
        }
    }
    Ok(())
}

/// Put a failed or finished meeting back in the transcription queue.
#[tauri::command]
pub fn retranscribe_meeting(app: AppHandle, db: State<'_, Db>, id: i64) -> CommandResult<()> {
    let meeting = find(&db, id)?;
    if matches!(meeting.status.as_str(), "recording" | "transcribing") {
        return Err(CommandError::new("busy", "this meeting is busy"));
    }
    db.set_meeting_status(id, "queued", None)?;
    if let Some(t) = app.try_state::<Transcriber>() {
        t.wake();
    }
    Ok(())
}

#[tauri::command]
pub fn search_transcripts(db: State<'_, Db>, query: String) -> CommandResult<Vec<SearchHit>> {
    Ok(db.search_transcripts(&query, 100)?)
}

/// Show a meeting's audio file in Finder / Explorer / the file manager.
/// Takes a meeting id, not a path, so the UI cannot ask to reveal arbitrary files.
#[tauri::command]
pub fn reveal_meeting(app: AppHandle, db: State<'_, Db>, id: i64) -> CommandResult<()> {
    let path = find(&db, id)?
        .audio_path
        .ok_or_else(|| CommandError::new("not_found", "meeting has no audio file"))?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| CommandError::new("opener", e.to_string()))
}
