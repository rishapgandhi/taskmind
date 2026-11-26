//! Runs Whisper over a recording in 5-minute chunks (bounded memory) and labels each
//! segment "you" or "others" by comparing the mic and system tracks.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::TranscribeError;
use crate::audio::decode::{Decimator3, OpusFileReader, envelope};
use crate::audio::{FRAME, SAMPLE_RATE};
use crate::db::SegmentRow;

const RATE_16K: usize = 16_000;
/// 5 minutes of 16 kHz audio per Whisper call (about 19 MB of f32).
const CHUNK_SAMPLES: usize = RATE_16K * 300;
/// Envelope block for speaker attribution: 100 ms at 48 kHz.
const ENV_BLOCK: usize = (SAMPLE_RATE / 10) as usize;
/// Below this RMS on both tracks, a "segment" is a Whisper hallucination on silence.
const SILENCE_RMS: f32 = 0.002;

pub struct Job<'a> {
    pub model_path: &'a Path,
    pub audio_path: &'a Path,
    pub mic_track: Option<PathBuf>,
    pub system_track: Option<PathBuf>,
    /// "auto" or an ISO code like "en", "hi".
    pub language: &'a str,
    pub threads: i32,
    pub duration_ms: Option<u64>,
}

pub struct Output {
    pub segments: Vec<SegmentRow>,
    pub language: Option<String>,
}

pub fn run(
    job: &Job<'_>,
    on_progress: Arc<dyn Fn(f32) + Send + Sync>,
    abort: Arc<AtomicBool>,
) -> Result<Output, TranscribeError> {
    let mic_env = job.mic_track.as_deref().and_then(envelope_or_none);
    let sys_env = job.system_track.as_deref().and_then(envelope_or_none);

    let ctx = WhisperContext::new_with_params(job.model_path, WhisperContextParameters::default())
        .map_err(|e| TranscribeError::Whisper(e.to_string()))?;
    let mut state = ctx
        .create_state()
        .map_err(|e| TranscribeError::Whisper(e.to_string()))?;

    let mut reader = OpusFileReader::open(job.audio_path)?;
    let mut decimator = Decimator3::default();
    let mut raw = Vec::with_capacity(FRAME * 50);
    let mut chunk: Vec<f32> = Vec::with_capacity(CHUNK_SAMPLES + RATE_16K);
    let mut offset_ms: i64 = 0;
    let mut language: Option<String> = match job.language {
        "" | "auto" => None,
        code => Some(code.to_string()),
    };
    let total_ms = job.duration_ms.unwrap_or(0).max(1) as f32;
    let mut segments = Vec::new();
    let mut more = true;

    while more {
        chunk.clear();
        while more && chunk.len() < CHUNK_SAMPLES {
            raw.clear();
            more = reader.read_into(&mut raw, FRAME * 50)?;
            decimator.process(&raw, &mut chunk);
        }
        if abort.load(Ordering::Relaxed) {
            return Err(TranscribeError::Aborted);
        }
        if chunk.len() < RATE_16K / 2 {
            break; // less than half a second left
        }

        let chunk_ms = (chunk.len() * 1000 / RATE_16K) as f32;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(job.threads);
        params.set_language(Some(language.as_deref().unwrap_or("auto")));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        params.set_suppress_blank(true);
        let progress = on_progress.clone();
        let base = offset_ms as f32;
        params.set_progress_callback_safe(move |pct: i32| {
            progress(((base + chunk_ms * pct as f32 / 100.0) / total_ms).min(1.0));
        });
        // whisper-rs 0.16's `set_abort_callback_safe` casts its closure to the wrong type and
        // aborts at random, so we pass a plain function that reads our flag instead.
        // SAFETY: the pointer is to the AtomicBool inside `abort`, which outlives `state.full`.
        unsafe {
            params.set_abort_callback(Some(abort_trampoline));
            params.set_abort_callback_user_data(Arc::as_ptr(&abort) as *mut std::ffi::c_void);
        }

        state.full(params, &chunk).map_err(|e| {
            if abort.load(Ordering::Relaxed) {
                TranscribeError::Aborted
            } else {
                TranscribeError::Whisper(e.to_string())
            }
        })?;

        if language.is_none() {
            language = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(String::from);
        }

        for seg in state.as_iter() {
            let text = seg
                .to_str_lossy()
                .map(|t| t.trim().to_string())
                .unwrap_or_default();
            if is_noise(&text) {
                continue;
            }
            let start_ms = offset_ms + seg.start_timestamp() * 10;
            let end_ms = (offset_ms + seg.end_timestamp() * 10).max(start_ms);
            let speaker = attribute(mic_env.as_deref(), sys_env.as_deref(), start_ms, end_ms);
            if speaker == Speaker::Silence {
                continue;
            }
            segments.push(SegmentRow {
                start_ms,
                end_ms,
                speaker: speaker.label().to_string(),
                text,
            });
        }
        offset_ms += (chunk.len() * 1000 / RATE_16K) as i64;
        on_progress((offset_ms as f32 / total_ms).min(1.0));
    }

    Ok(Output { segments, language })
}

unsafe extern "C" fn abort_trampoline(user_data: *mut std::ffi::c_void) -> bool {
    // SAFETY: set up in `run`; points to a live AtomicBool for the duration of `full()`.
    unsafe { (*(user_data as *const AtomicBool)).load(Ordering::Relaxed) }
}

fn envelope_or_none(path: &Path) -> Option<Vec<f32>> {
    if !path.exists() {
        return None;
    }
    match envelope(path, ENV_BLOCK) {
        Ok(env) => Some(env),
        Err(err) => {
            tracing::warn!(%err, path = %path.display(), "could not read track, speaker labels off");
            None
        }
    }
}

/// Whisper's markers for non-speech, e.g. "[BLANK_AUDIO]", "(music)", "[ Silence ]".
fn is_noise(text: &str) -> bool {
    let t = text.trim();
    t.is_empty()
        || (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('*') && t.ends_with('*'))
}

#[derive(Debug, PartialEq, Eq)]
enum Speaker {
    You,
    Others,
    Unknown,
    Silence,
}

impl Speaker {
    fn label(&self) -> &'static str {
        match self {
            Speaker::You => "you",
            Speaker::Others => "others",
            Speaker::Unknown | Speaker::Silence => "unknown",
        }
    }
}

fn mean(env: &[f32], start_ms: i64, end_ms: i64) -> Option<f32> {
    let from = usize::try_from(start_ms / 100).ok()?;
    let to = usize::try_from(end_ms / 100)
        .ok()?
        .min(env.len().saturating_sub(1));
    if from > to || from >= env.len() {
        return None;
    }
    let slice = &env[from..=to];
    Some(slice.iter().sum::<f32>() / slice.len() as f32)
}

/// The system track is clean (digital), while the mic also hears the speakers. So "others"
/// wins whenever the system track carries a fair share of the energy.
fn attribute(mic: Option<&[f32]>, sys: Option<&[f32]>, start_ms: i64, end_ms: i64) -> Speaker {
    let m = mic.and_then(|e| mean(e, start_ms, end_ms));
    let s = sys.and_then(|e| mean(e, start_ms, end_ms));
    match (m, s) {
        (Some(m), Some(s)) if m < SILENCE_RMS && s < SILENCE_RMS => Speaker::Silence,
        (Some(m), Some(s)) => {
            if s >= m * 0.6 {
                Speaker::Others
            } else {
                Speaker::You
            }
        }
        (Some(m), None) if m >= SILENCE_RMS => Speaker::You,
        (None, Some(s)) if s >= SILENCE_RMS => Speaker::Others,
        _ => Speaker::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_markers_are_dropped() {
        for t in ["[BLANK_AUDIO]", "(music)", "[ Silence ]", "*laughs*", "  "] {
            assert!(is_noise(t), "{t}");
        }
        assert!(!is_noise("Hello (everyone)"));
    }

    #[test]
    fn attribution() {
        let quiet = vec![0.0005; 50];
        let loud = vec![0.1; 50];
        let bleed = vec![0.03; 50];
        assert_eq!(attribute(Some(&loud), Some(&quiet), 0, 2000), Speaker::You);
        assert_eq!(
            attribute(Some(&bleed), Some(&loud), 0, 2000),
            Speaker::Others
        );
        assert_eq!(
            attribute(Some(&quiet), Some(&quiet), 0, 2000),
            Speaker::Silence
        );
        assert_eq!(attribute(None, None, 0, 2000), Speaker::Unknown);
        assert_eq!(attribute(Some(&loud), None, 0, 2000), Speaker::You);
        // Past the end of the envelope.
        assert_eq!(
            attribute(Some(&loud), Some(&quiet), 900_000, 901_000),
            Speaker::Unknown
        );
    }
}

#[cfg(test)]
mod hardware_tests {
    use super::*;

    /// End to end on a real recording. Needs a model and a recording made by
    /// `audio::session::tests::records_real_devices` with TASKMIND_TEST_OUT set:
    /// `TASKMIND_TEST_MODEL=.../ggml-tiny.bin TASKMIND_TEST_OUT=dir cargo test transcribes_real_recording -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a Whisper model and a real recording"]
    fn transcribes_real_recording() {
        let model = PathBuf::from(std::env::var("TASKMIND_TEST_MODEL").unwrap());
        let dir = PathBuf::from(std::env::var("TASKMIND_TEST_OUT").unwrap());
        let job = Job {
            model_path: &model,
            audio_path: &dir.join("mix.ogg"),
            mic_track: Some(dir.join("mic.ogg")),
            system_track: Some(dir.join("system.ogg")),
            language: "auto",
            threads: 4,
            duration_ms: Some(10_000),
        };
        let started = std::time::Instant::now();
        let out = run(&job, Arc::new(|_| {}), Arc::new(AtomicBool::new(false))).unwrap();
        eprintln!("language: {:?}, took {:?}", out.language, started.elapsed());
        for s in &out.segments {
            eprintln!(
                "[{:>6} - {:>6}] {:<7} {}",
                s.start_ms, s.end_ms, s.speaker, s.text
            );
        }
        assert!(!out.segments.is_empty());
    }
}
