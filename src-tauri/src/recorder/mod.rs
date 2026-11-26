//! Recorder: ties the state machine to audio sessions, the database, events, tray and widget.
//! Every recorder action (command, tray, shortcut, app exit) goes through `dispatch`.

mod machine;

use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub use machine::{Action, Status};
use machine::{InvalidTransition, Machine};

use crate::audio::{Levels, Session, SessionConfig, SourceStatus};
use crate::db::Db;
use crate::{tray, windows};

pub const STATE_EVENT: &str = "recorder://state";
pub const LEVEL_EVENT: &str = "recorder://level";
pub const ERROR_EVENT: &str = "recorder://error";
pub const CONSENT_EVENT: &str = "recorder://consent-reminder";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: Status,
    pub started_at: Option<DateTime<Utc>>,
    pub elapsed_ms: u64,
    pub meeting_id: Option<i64>,
    pub sources: Option<SourceStatus>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error(transparent)]
    InvalidTransition(#[from] InvalidTransition),
    #[error("recorder lock was poisoned by a crashed thread")]
    Poisoned,
    #[error("could not start recording: {0}")]
    Audio(String),
    #[error("could not save recording: {0}")]
    Storage(String),
}

#[derive(Default)]
struct Inner {
    machine: Machine,
    session: Option<Session>,
    meeting_id: Option<i64>,
    sources: Option<SourceStatus>,
}

impl Inner {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            status: self.machine.status(),
            started_at: self.machine.started_at(),
            elapsed_ms: self.machine.elapsed_ms(),
            meeting_id: self.meeting_id,
            sources: self.sources.clone(),
        }
    }
}

#[derive(Default)]
pub struct RecorderHandle(Mutex<Inner>);

impl RecorderHandle {
    fn lock(&self) -> Result<MutexGuard<'_, Inner>, RecorderError> {
        self.0.lock().map_err(|_| RecorderError::Poisoned)
    }

    pub fn snapshot(&self) -> Result<Snapshot, RecorderError> {
        Ok(self.lock()?.snapshot())
    }
}

pub fn dispatch(app: &AppHandle, action: Action) -> Result<Snapshot, RecorderError> {
    match apply(app, action) {
        Ok((snapshot, just_started)) => {
            tracing::info!(?action, status = ?snapshot.status, "recorder state changed");
            emit(app, STATE_EVENT, &snapshot);
            tray::sync(app, &snapshot);
            if just_started {
                // Principle 5: the indicator must be visible whenever we record.
                windows::show_widget(app);
                if consent_reminder_enabled(app) {
                    emit(app, CONSENT_EVENT, ());
                }
            }
            Ok(snapshot)
        }
        Err(err) => {
            tracing::warn!(?action, %err, "recorder action failed");
            if !matches!(err, RecorderError::InvalidTransition(_)) {
                // Shortcut and tray users have no other place to see the error.
                emit(app, ERROR_EVENT, err.to_string());
                windows::show_widget(app);
            }
            Err(err)
        }
    }
}

/// Finish any recording before the app exits so the files are closed properly.
pub fn shutdown(app: &AppHandle) {
    let active = app
        .try_state::<RecorderHandle>()
        .and_then(|h| h.snapshot().ok())
        .is_some_and(|s| s.status != Status::Idle);
    if active && let Err(err) = dispatch(app, Action::Stop) {
        tracing::error!(%err, "could not stop recording on exit");
    }
}

fn apply(app: &AppHandle, action: Action) -> Result<(Snapshot, bool), RecorderError> {
    let handle = app.state::<RecorderHandle>();
    let mut inner = handle.lock()?;
    let action = inner.machine.check(action)?;
    match action {
        Action::Start => {
            start_session(app, &mut inner)?;
            inner.machine.apply(Action::Start)?;
        }
        Action::Pause => {
            inner.machine.apply(Action::Pause)?;
            if let Some(session) = &inner.session {
                session.pause();
            }
        }
        Action::Resume => {
            inner.machine.apply(Action::Resume)?;
            if let Some(session) = &inner.session {
                session.resume();
            }
        }
        Action::Stop | Action::Toggle => {
            inner.machine.apply(Action::Stop)?;
            finish_session(app, &mut inner);
        }
    }
    Ok((inner.snapshot(), action == Action::Start))
}

fn start_session(app: &AppHandle, inner: &mut Inner) -> Result<(), RecorderError> {
    let db = app.state::<Db>();
    let root = crate::storage::recordings_root(app);

    let now = Local::now();
    let stamp = now.format("%Y-%m-%d_%H%M%S").to_string();
    let mix_path = root
        .join(now.format("%Y-%m").to_string())
        .join(format!("{stamp}.ogg"));
    let (mic_track_path, system_track_path) = crate::audio::track_paths(&mix_path);
    let config = SessionConfig {
        mix_path: mix_path.clone(),
        mic_track_path,
        system_track_path,
    };

    let meeting_id = db
        .create_meeting(&Utc::now().to_rfc3339(), &mix_path.display().to_string())
        .map_err(|e| RecorderError::Storage(e.to_string()))?;

    let level_app = app.clone();
    let on_level = move |levels: Levels| emit(&level_app, LEVEL_EVENT, levels);

    match Session::start(config, on_level) {
        Ok(session) => {
            inner.sources = Some(session.sources.clone());
            inner.session = Some(session);
            inner.meeting_id = Some(meeting_id);
            Ok(())
        }
        Err(err) => {
            if let Err(db_err) = db.delete_meeting(meeting_id) {
                tracing::warn!(%db_err, "could not remove failed meeting row");
            }
            let _ = std::fs::remove_file(&mix_path);
            Err(RecorderError::Audio(err.to_string()))
        }
    }
}

fn finish_session(app: &AppHandle, inner: &mut Inner) {
    inner.sources = None;
    let (Some(session), Some(meeting_id)) = (inner.session.take(), inner.meeting_id.take()) else {
        return;
    };
    let db = app.state::<Db>();
    let ended_at = Utc::now().to_rfc3339();
    let saved = match session.stop() {
        Ok(summary) => db.finish_meeting(
            meeting_id,
            &ended_at,
            i64::try_from(summary.duration_ms.div_ceil(1000)).unwrap_or(i64::MAX),
            i64::try_from(summary.bytes).unwrap_or(i64::MAX),
        ),
        Err(err) => {
            tracing::error!(%err, meeting_id, "recording failed while finishing");
            db.fail_meeting(meeting_id, &ended_at)
        }
    };
    if let Err(err) = saved {
        tracing::error!(%err, meeting_id, "could not update meeting row");
    }
    emit(app, "meetings://changed", meeting_id);
    if let Some(t) = app.try_state::<crate::transcribe::Transcriber>() {
        t.wake();
    }
}

fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(%err, event, "failed to emit event");
    }
}

fn consent_reminder_enabled(app: &AppHandle) -> bool {
    app.try_state::<Db>()
        .is_none_or(|db| crate::settings::get_bool(&db, crate::settings::CONSENT_REMINDER, true))
}
