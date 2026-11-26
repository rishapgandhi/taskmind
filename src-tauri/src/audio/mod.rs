//! Audio capture and encoding.
//!
//! - `capture`: opens the mic and system-audio streams with cpal and feeds lock-free ring buffers.
//! - `platform`: picks the system-audio device per OS (loopback on macOS/Windows, monitor source on Linux).
//! - `resample`, `mixer`: bring both sources to 48 kHz mono and mix them.
//! - `encoder`: writes Ogg Opus files page by page so a crash keeps almost all audio.
//! - `session`: the recording thread that ties it together.

mod capture;
pub mod decode;
mod encoder;
pub mod mixer;
mod resample;
mod session;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(not(target_os = "linux"))]
#[path = "loopback.rs"]
mod platform;

pub use capture::SourceKind;
pub use session::{Levels, Session, SessionConfig, SourceStatus};

/// Separate mic and system tracks live next to the mixed file: `<dir>/.tracks/<stem>-mic.ogg`.
pub fn track_paths(mix_path: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = mix_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(".tracks");
    let stem = mix_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    (
        dir.join(format!("{stem}-mic.ogg")),
        dir.join(format!("{stem}-system.ogg")),
    )
}

/// Internal and file sample rate. Opus works natively at 48 kHz.
pub const SAMPLE_RATE: u32 = 48_000;
/// One Opus frame: 20 ms at 48 kHz.
pub const FRAME: usize = 960;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no {0} device found")]
    NoDevice(SourceKind),
    #[error("{0}")]
    Unsupported(String),
    #[error("audio device error: {0}")]
    Device(String),
    #[error("could not record from microphone ({mic}) or system audio ({system})")]
    NoSources { mic: String, system: String },
    #[error("encoder error: {0}")]
    Encoder(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("recording thread stopped unexpectedly")]
    ThreadGone,
}

impl From<cpal::Error> for AudioError {
    fn from(err: cpal::Error) -> Self {
        AudioError::Device(err.to_string())
    }
}

impl From<opus::Error> for AudioError {
    fn from(err: opus::Error) -> Self {
        AudioError::Encoder(err.to_string())
    }
}
