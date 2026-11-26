//! Background transcription queue. One meeting at a time, using half the CPU cores,
//! so the computer stays usable (principle 1). Jobs survive restarts through `meetings.status`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::engine::{self, Job};
use super::{TranscribeError, models};
use crate::audio;
use crate::db::{Db, MeetingRow};
use crate::settings;

pub const STATUS_EVENT: &str = "transcribe://status";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    Idle,
    Working,
    Paused,
    NeedsModel,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatus {
    pub state: WorkerState,
    pub meeting_id: Option<i64>,
    pub progress: f32,
    pub model: Option<String>,
}

enum Msg {
    Wake,
    Shutdown,
}

pub struct Transcriber {
    tx: Sender<Msg>,
    abort: Arc<AtomicBool>,
    status: Arc<Mutex<WorkerStatus>>,
    models_dir: PathBuf,
}

impl Transcriber {
    pub fn start(app: AppHandle, models_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let abort = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(WorkerStatus {
            state: WorkerState::Idle,
            meeting_id: None,
            progress: 0.0,
            model: None,
        }));
        let ctx = Ctx {
            app,
            models_dir: models_dir.clone(),
            abort: abort.clone(),
            status: status.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name("taskmind-transcriber".into())
            .spawn(move || run(ctx, rx));
        if let Err(err) = spawned {
            tracing::error!(%err, "could not start transcription thread");
        }
        let me = Self {
            tx,
            abort,
            status,
            models_dir,
        };
        me.wake();
        me
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    /// Check the queue now (new recording, setting changed, model downloaded).
    pub fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    /// Stop the current job. It goes back to the queue.
    pub fn abort_current(&self) {
        self.abort.store(true, Ordering::Relaxed);
    }

    pub fn shutdown(&self) {
        self.abort_current();
        let _ = self.tx.send(Msg::Shutdown);
    }

    pub fn status(&self) -> Option<WorkerStatus> {
        self.status.lock().ok().map(|s| s.clone())
    }
}

struct Ctx {
    app: AppHandle,
    models_dir: PathBuf,
    abort: Arc<AtomicBool>,
    status: Arc<Mutex<WorkerStatus>>,
}

impl Ctx {
    fn set_status(
        &self,
        state: WorkerState,
        meeting_id: Option<i64>,
        progress: f32,
        model: Option<&str>,
    ) {
        let snapshot = WorkerStatus {
            state,
            meeting_id,
            progress,
            model: model.map(String::from),
        };
        if let Ok(mut s) = self.status.lock() {
            *s = snapshot.clone();
        }
        let _ = self.app.emit(STATUS_EVENT, snapshot);
    }

    fn meetings_changed(&self, id: i64) {
        let _ = self.app.emit("meetings://changed", id);
    }
}

fn run(ctx: Ctx, rx: Receiver<Msg>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
        }
        // Collapse a burst of wake-ups into one pass.
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, Msg::Shutdown) {
                return;
            }
        }
        while process_next(&ctx) {}
    }
}

/// Returns true if a meeting was handled and the queue should be checked again.
fn process_next(ctx: &Ctx) -> bool {
    let db = ctx.app.state::<Db>();
    if settings::get_bool(&db, settings::TRANSCRIBE_PAUSED, false) {
        ctx.set_status(WorkerState::Paused, None, 0.0, None);
        return false;
    }
    let meeting = match db.next_queued() {
        Ok(Some(m)) => m,
        Ok(None) => {
            ctx.set_status(WorkerState::Idle, None, 0.0, None);
            return false;
        }
        Err(err) => {
            tracing::error!(%err, "could not read transcription queue");
            return false;
        }
    };
    let model_id = settings::get_string(&db, settings::TRANSCRIBE_MODEL)
        .unwrap_or_else(|| models::recommended().to_string());
    let model = models::find(&model_id)
        .or_else(|| models::find(models::recommended()))
        .unwrap_or(&models::MODELS[0]);
    if !models::is_downloaded(&ctx.models_dir, model) {
        ctx.set_status(WorkerState::NeedsModel, None, 0.0, Some(model.id));
        return false;
    }

    ctx.abort.store(false, Ordering::Relaxed);
    if let Err(err) = db.set_meeting_status(meeting.id, "transcribing", None) {
        tracing::error!(%err, "could not mark meeting as transcribing");
        return false;
    }
    ctx.meetings_changed(meeting.id);
    ctx.set_status(WorkerState::Working, Some(meeting.id), 0.0, Some(model.id));
    tracing::info!(
        meeting = meeting.id,
        model = model.id,
        "transcription started"
    );

    let language =
        settings::get_string(&db, settings::TRANSCRIBE_LANGUAGE).unwrap_or_else(|| "auto".into());
    let result = transcribe(ctx, &meeting, model, &language);

    let keep_going = match result {
        Ok(out) => {
            let count = out.segments.len();
            match db.save_transcript(meeting.id, &out.segments, out.language.as_deref(), model.id) {
                Ok(()) => {
                    tracing::info!(meeting = meeting.id, segments = count, "transcription done");
                    crate::notes::maybe_auto_generate(&ctx.app, meeting.id);
                    if let Some(path) = &meeting.audio_path {
                        let (mic, sys) = audio::track_paths(Path::new(path));
                        let _ = std::fs::remove_file(mic);
                        let _ = std::fs::remove_file(sys);
                    }
                }
                Err(err) => {
                    tracing::error!(%err, meeting = meeting.id, "could not save transcript")
                }
            }
            true
        }
        Err(TranscribeError::Aborted) => {
            tracing::info!(meeting = meeting.id, "transcription stopped, back in queue");
            let _ = db.set_meeting_status(meeting.id, "queued", None);
            false
        }
        Err(err) => {
            tracing::error!(%err, meeting = meeting.id, "transcription failed");
            let _ = db.set_meeting_status(meeting.id, "failed", Some(&err.to_string()));
            true
        }
    };
    ctx.meetings_changed(meeting.id);
    ctx.set_status(WorkerState::Idle, None, 0.0, None);
    keep_going
}

fn transcribe(
    ctx: &Ctx,
    meeting: &MeetingRow,
    model: &models::ModelInfo,
    language: &str,
) -> Result<engine::Output, TranscribeError> {
    let audio_path = meeting
        .audio_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .ok_or_else(|| TranscribeError::Io(std::io::Error::other("audio file is missing")))?;
    let (mic, sys) = audio::track_paths(&audio_path);
    let model_path = models::path(&ctx.models_dir, model);
    let threads = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 8))
        .unwrap_or(2);

    let app = ctx.app.clone();
    let status = ctx.status.clone();
    let meeting_id = meeting.id;
    let model_id = model.id.to_string();
    let last = Arc::new(Mutex::new(0.0f32));
    let on_progress = Arc::new(move |p: f32| {
        let Ok(mut last) = last.lock() else { return };
        if p - *last < 0.01 && p < 1.0 {
            return;
        }
        *last = p;
        let snapshot = WorkerStatus {
            state: WorkerState::Working,
            meeting_id: Some(meeting_id),
            progress: p,
            model: Some(model_id.clone()),
        };
        if let Ok(mut s) = status.lock() {
            *s = snapshot.clone();
        }
        let _ = app.emit(STATUS_EVENT, snapshot);
    });

    let job = Job {
        model_path: &model_path,
        audio_path: &audio_path,
        mic_track: Some(mic),
        system_track: Some(sys),
        language,
        threads: i32::try_from(threads).unwrap_or(2),
        duration_ms: meeting
            .duration_s
            .map(|s| u64::try_from(s).unwrap_or(0) * 1000),
    };
    engine::run(&job, on_progress, ctx.abort.clone())
}
