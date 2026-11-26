//! Local transcription with whisper.cpp.
//!
//! - `models`: catalog, verified download, delete.
//! - `engine`: Whisper over a recording in chunks, with "you"/"others" labels.
//! - `worker`: one background thread that transcribes queued meetings, oldest first.

pub mod engine;
pub mod models;
mod worker;

pub use worker::{Transcriber, WorkerStatus};

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error("transcription was stopped")]
    Aborted,
    #[error("whisper: {0}")]
    Whisper(String),
    #[error("audio: {0}")]
    Audio(#[from] crate::audio::AudioError),
    #[error("download failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    BadDownload(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
}
