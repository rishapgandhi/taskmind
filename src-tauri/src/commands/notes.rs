//! AI notes commands.

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::db::{DayActionItem, Db, StoredNotes};
use crate::error::{CommandError, CommandResult};
use crate::notes::{self, Notes, NotesError, ProviderKind};
use crate::timesheet;

fn err(e: NotesError) -> CommandError {
    CommandError::new("notes", e.to_string())
}

#[tauri::command]
pub fn get_notes(db: State<'_, Db>, id: i64) -> CommandResult<Option<StoredNotes>> {
    Ok(db.get_notes(id)?)
}

#[tauri::command]
pub async fn generate_notes(app: AppHandle, id: i64) -> CommandResult<Notes> {
    notes::generate_for_meeting(&app, id).await.map_err(err)
}

#[tauri::command]
pub fn save_notes(db: State<'_, Db>, id: i64, notes: Notes) -> CommandResult<()> {
    if notes.summary.len() > 50_000 || notes.action_items.len() > 500 || notes.decisions.len() > 500
    {
        return Err(CommandError::new("invalid", "notes are too long"));
    }
    db.get_meeting(id)?
        .ok_or_else(|| CommandError::new("not_found", "meeting not found"))?;
    Ok(db.save_notes(id, &notes, None)?)
}

fn kind(provider: &str) -> CommandResult<ProviderKind> {
    ProviderKind::parse(Some(provider))
        .ok_or_else(|| CommandError::new("invalid", "unknown provider"))
}

/// Store (or remove, with an empty key) an API key in the OS keychain.
#[tauri::command]
pub fn set_notes_key(provider: String, key: String) -> CommandResult<()> {
    notes::keychain_set(kind(&provider)?, Some(&key)).map_err(err)
}

#[tauri::command]
pub fn has_notes_key(provider: String) -> CommandResult<bool> {
    Ok(notes::keychain_get(kind(&provider)?)
        .map_err(err)?
        .is_some())
}

#[tauri::command]
pub fn day_action_items(db: State<'_, Db>, date_str: String) -> CommandResult<Vec<DayActionItem>> {
    let day = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| CommandError::new("invalid", "date must be YYYY-MM-DD"))?;
    let (from, to) = timesheet::day_bounds(day);
    let iso = |d: chrono::DateTime<Utc>| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    Ok(db.action_items_between(&iso(from), &iso(to))?)
}

#[tauri::command]
pub async fn check_notes_provider(app: AppHandle) -> CommandResult<String> {
    use tauri::Manager;
    let db = app.state::<Db>();
    notes::check_provider(&db).await.map_err(err)
}
