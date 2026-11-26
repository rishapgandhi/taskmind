//! Transcription model and queue commands.

use std::collections::HashSet;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use crate::error::{CommandError, CommandResult};
use crate::transcribe::{Transcriber, WorkerStatus, models};

/// Model ids being downloaded right now, so a double click does not start two downloads.
#[derive(Default)]
pub struct Downloads(Mutex<HashSet<&'static str>>);

fn model(id: &str) -> CommandResult<&'static models::ModelInfo> {
    models::find(id).ok_or_else(|| CommandError::new("not_found", format!("unknown model '{id}'")))
}

#[tauri::command]
pub fn list_models(t: State<'_, Transcriber>) -> Vec<models::ModelStatus> {
    models::list(t.models_dir())
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> CommandResult<()> {
    let info = model(&id)?;
    {
        let downloads = app.state::<Downloads>();
        let mut active = downloads
            .0
            .lock()
            .map_err(|_| CommandError::new("app", "download lock poisoned"))?;
        if !active.insert(info.id) {
            return Err(CommandError::new(
                "busy",
                "this model is already downloading",
            ));
        }
    }
    let dir = app.state::<Transcriber>().models_dir().to_path_buf();
    let result = models::download(&app, &dir, info).await;
    if let Ok(mut active) = app.state::<Downloads>().0.lock() {
        active.remove(info.id);
    }
    result.map_err(|e| CommandError::new("download", e.to_string()))?;
    app.state::<Transcriber>().wake();
    Ok(())
}

#[tauri::command]
pub fn delete_model(t: State<'_, Transcriber>, id: String) -> CommandResult<()> {
    models::delete(t.models_dir(), model(&id)?).map_err(|e| CommandError::new("io", e.to_string()))
}

#[tauri::command]
pub fn transcription_status(t: State<'_, Transcriber>) -> Option<WorkerStatus> {
    t.status()
}
