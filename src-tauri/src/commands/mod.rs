//! Thin Tauri command handlers. Validate input, call a module, map errors.
//! Every command here must also be listed in `build.rs` and granted in `capabilities/`.

use serde::Serialize;
use tauri::{AppHandle, State};

pub mod data;
pub mod meetings;
pub mod notes;
pub mod transcribe;
pub mod work;

use tauri::Manager;

use crate::db::{Db, Table};
use crate::error::{CommandError, CommandResult};
use crate::recorder::{self, Action, RecorderHandle, Snapshot};
use crate::{settings, shortcuts, windows};

#[tauri::command]
pub fn recorder_state(handle: State<'_, RecorderHandle>) -> CommandResult<Snapshot> {
    Ok(handle.snapshot()?)
}

#[tauri::command]
pub fn recorder_start(app: AppHandle) -> CommandResult<Snapshot> {
    Ok(recorder::dispatch(&app, Action::Start)?)
}

#[tauri::command]
pub fn recorder_stop(app: AppHandle) -> CommandResult<Snapshot> {
    Ok(recorder::dispatch(&app, Action::Stop)?)
}

#[tauri::command]
pub fn recorder_pause(app: AppHandle) -> CommandResult<Snapshot> {
    Ok(recorder::dispatch(&app, Action::Pause)?)
}

#[tauri::command]
pub fn recorder_resume(app: AppHandle) -> CommandResult<Snapshot> {
    Ok(recorder::dispatch(&app, Action::Resume)?)
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    windows::show_main(&app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    db_path: Option<String>,
    schema_version: usize,
    meeting_count: i64,
    activity_count: i64,
    shortcuts: shortcuts::Labels,
}

#[tauri::command]
pub fn app_info(app: AppHandle, db: State<'_, Db>) -> CommandResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        db_path: db.path().map(|p| p.display().to_string()),
        schema_version: db.schema_version()?,
        meeting_count: db.count(Table::Meetings)?,
        activity_count: db.count(Table::Activities)?,
        shortcuts: shortcuts::labels(),
    })
}

fn check_key(key: &str) -> CommandResult<()> {
    if settings::ALLOWED.contains(&key) {
        Ok(())
    } else {
        Err(CommandError::new(
            "invalid_setting",
            format!("unknown setting '{key}'"),
        ))
    }
}

#[tauri::command]
pub fn get_setting(db: State<'_, Db>, key: String) -> CommandResult<Option<serde_json::Value>> {
    check_key(&key)?;
    Ok(db.get_setting(&key)?)
}

#[tauri::command]
pub fn set_setting(
    app: AppHandle,
    db: State<'_, Db>,
    key: String,
    value: serde_json::Value,
) -> CommandResult<()> {
    check_key(&key)?;
    if key == settings::STORAGE_RECORDINGS_DIR
        && let Some(dir) = value.as_str()
        && !std::path::Path::new(dir).is_absolute()
    {
        return Err(CommandError::new(
            "invalid",
            "folder must be an absolute path",
        ));
    }
    db.set_setting(&key, &value)?;
    if key == settings::STORAGE_RECORDINGS_DIR {
        crate::storage::allow_playback(&app);
    }
    if (key.starts_with("tracking.") || key.starts_with("meetings."))
        && let Some(t) = app.try_state::<crate::tracker::Tracker>()
    {
        t.wake();
    }
    if key.starts_with("transcription.")
        && let Some(t) = app.try_state::<crate::transcribe::Transcriber>()
    {
        // Pausing or switching model stops the current job. It goes back in the queue.
        if key == settings::TRANSCRIBE_PAUSED && value == serde_json::Value::Bool(true)
            || key == settings::TRANSCRIBE_MODEL
        {
            t.abort_current();
        }
        t.wake();
    }
    Ok(())
}
