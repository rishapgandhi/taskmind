//! Backup, restore, v1 import and storage location.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::Db;
use crate::error::{CommandError, CommandResult};
use crate::{import_v1, storage};

fn io(e: impl ToString) -> CommandError {
    CommandError::new("io", e.to_string())
}

/// Save a backup zip to Downloads and show it.
#[tauri::command]
pub fn backup_now(app: AppHandle, db: State<'_, Db>) -> CommandResult<String> {
    let dir = dirs::download_dir()
        .or_else(|| app.path().app_data_dir().ok())
        .ok_or_else(|| io("no folder to save to"))?;
    let path = storage::backup(&db, &dir).map_err(io)?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}

/// Check the backup, then restart; it replaces the database on start (the old one is kept).
#[tauri::command]
pub fn restore_backup(app: AppHandle, path: String) -> CommandResult<()> {
    let data_dir = app.path().app_data_dir().map_err(io)?;
    storage::stage_restore(std::path::Path::new(&path), &data_dir)
        .map_err(|e| CommandError::new("invalid", e))?;
    crate::recorder::shutdown(&app);
    app.restart();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatus {
    available: bool,
    done_at: Option<String>,
}

#[tauri::command]
pub fn import_v1_status(db: State<'_, Db>) -> ImportStatus {
    ImportStatus {
        available: import_v1::available(),
        done_at: crate::settings::get_string(&db, import_v1::DONE_SETTING),
    }
}

#[tauri::command]
pub fn run_import_v1(db: State<'_, Db>) -> CommandResult<import_v1::ImportSummary> {
    if crate::settings::get_string(&db, import_v1::DONE_SETTING).is_some() {
        return Err(CommandError::new(
            "done",
            "TaskMind v1 data was already imported",
        ));
    }
    let summary = import_v1::import(
        &db,
        import_v1::v1_db_path().as_deref(),
        import_v1::v1_projects_path().as_deref(),
    )
    .map_err(|e| CommandError::new("import", e))?;
    if summary.found {
        db.set_setting(
            import_v1::DONE_SETTING,
            &serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
        )?;
    }
    Ok(summary)
}

#[tauri::command]
pub fn recordings_folder(app: AppHandle) -> String {
    storage::recordings_root(&app).display().to_string()
}
