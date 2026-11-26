//! Serializable error returned by every Tauri command.

use serde::Serialize;

use crate::db::DbError;
use crate::recorder::RecorderError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<DbError> for CommandError {
    fn from(err: DbError) -> Self {
        tracing::error!(%err, "database command failed");
        Self::new("db", err.to_string())
    }
}

impl From<RecorderError> for CommandError {
    fn from(err: RecorderError) -> Self {
        Self::new("recorder", err.to_string())
    }
}

impl From<tauri::Error> for CommandError {
    fn from(err: tauri::Error) -> Self {
        tracing::error!(%err, "tauri command failed");
        Self::new("app", err.to_string())
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
