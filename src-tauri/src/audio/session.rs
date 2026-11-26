//! One recording session on its own thread.
//!
//! The thread owns the cpal streams (they are not `Send` on every OS), pulls samples from the
//! ring buffers every 20 ms, aligns both sources to the wall clock with a small latency buffer,
//! and writes three files: the mixed recording plus separate mic and system tracks
//! (the tracks let M2 label "You" vs "Others" without a diarization model).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::StreamTrait;
use ringbuf::traits::Consumer;
use serde::Serialize;

use super::capture::{self, Capture, SourceKind};
use super::encoder::OggOpusWriter;
use super::mixer;
use super::resample::Resampler;
use super::{AudioError, SAMPLE_RATE};

const TICK: Duration = Duration::from_millis(20);
/// Run this far behind real time so bursty device callbacks never cause gaps.
const LATENCY: Duration = Duration::from_millis(150);
/// If a source gets this far ahead of the clock (device clock drift), drop the excess.
const MAX_BACKLOG: usize = (SAMPLE_RATE as usize) / 4;
const LEVEL_EVERY: Duration = Duration::from_millis(100);
const MIX_BITRATE: i32 = 32_000;
const TRACK_BITRATE: i32 = 24_000;

pub struct SessionConfig {
    pub mix_path: PathBuf,
    pub mic_track_path: PathBuf,
    pub system_track_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Levels {
    pub mic: f32,
    pub system: f32,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    /// Device name when the source is recording.
    pub mic: Option<String>,
    pub system: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct SessionSummary {
    pub duration_ms: u64,
    pub bytes: u64,
}

enum Cmd {
    Pause,
    Resume,
    Stop,
}

pub struct Session {
    tx: Sender<Cmd>,
    thread: Option<JoinHandle<Result<SessionSummary, AudioError>>>,
    pub sources: SourceStatus,
}

impl Session {
    pub fn start(
        config: SessionConfig,
        on_level: impl Fn(Levels) + Send + 'static,
    ) -> Result<Self, AudioError> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("taskmind-recorder".into())
            .spawn(move || run(config, rx, ready_tx, on_level))?;

        match ready_rx.recv() {
            Ok(Ok(sources)) => Ok(Self {
                tx,
                thread: Some(thread),
                sources,
            }),
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(_) => match thread.join() {
                Ok(Err(err)) => Err(err),
                _ => Err(AudioError::ThreadGone),
            },
        }
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Cmd::Resume);
    }

    pub fn stop(mut self) -> Result<SessionSummary, AudioError> {
        self.finish()
    }

    fn finish(&mut self) -> Result<SessionSummary, AudioError> {
        let _ = self.tx.send(Cmd::Stop);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(result)) => result,
            _ => Err(AudioError::ThreadGone),
        }
    }
}

impl Drop for Session {
    /// Make sure files are closed properly if the app quits mid-recording.
    fn drop(&mut self) {
        if self.thread.is_some()
            && let Err(err) = self.finish()
        {
            tracing::error!(%err, "recording did not finish cleanly");
        }
    }
}

/// One source: capture stream, resampler, aligned buffer and its own track file.
struct Source {
    capture: Capture,
    resampler: Resampler,
    raw: Vec<f32>,
    aligned: Vec<f32>,
    track: OggOpusWriter,
    underruns: u64,
    reported_failure: bool,
}

impl Source {
    fn new(capture: Capture, track: OggOpusWriter) -> Self {
        let resampler = Resampler::new(capture.sample_rate, SAMPLE_RATE);
        Self {
            capture,
            resampler,
            raw: vec![0.0; SAMPLE_RATE as usize],
            aligned: Vec::with_capacity(SAMPLE_RATE as usize),
            track,
            underruns: 0,
            reported_failure: false,
        }
    }

    /// Move everything from the ring buffer into `aligned` at 48 kHz.
    fn pull(&mut self) {
        loop {
            let n = self.capture.consumer.pop_slice(&mut self.raw);
            if n == 0 {
                break;
            }
            self.resampler.process(&self.raw[..n], &mut self.aligned);
        }
    }

    /// Take exactly `need` samples into `out`, padding with silence or dropping drift.
    fn take(&mut self, need: usize, out: &mut Vec<f32>) {
        if self.aligned.len() > need + MAX_BACKLOG {
            let excess = self.aligned.len() - need - MAX_BACKLOG / 2;
            self.aligned.drain(..excess);
        }
        let have = self.aligned.len().min(need);
        out.clear();
        out.extend(self.aligned.drain(..have));
        if have < need {
            // Windows loopback sends nothing during silence; short underruns are normal.
            self.underruns += 1;
            out.resize(need, 0.0);
        }
    }

    fn discard_buffered(&mut self) {
        self.capture.consumer.clear();
        self.aligned.clear();
    }

    fn check_failure(&mut self, kind: SourceKind) {
        if !self.reported_failure && self.capture.failed.load(Ordering::Relaxed) {
            self.reported_failure = true;
            tracing::warn!(%kind, "audio source reported an error; it may be silent from now on");
        }
    }
}

fn open_source(kind: SourceKind, track_path: &Path) -> Result<Source, AudioError> {
    let capture = capture::open(kind)?;
    let track = OggOpusWriter::create(track_path, TRACK_BITRATE)?;
    Ok(Source::new(capture, track))
}

fn run(
    config: SessionConfig,
    rx: Receiver<Cmd>,
    ready: SyncSender<Result<SourceStatus, AudioError>>,
    on_level: impl Fn(Levels),
) -> Result<SessionSummary, AudioError> {
    let mut status = SourceStatus::default();

    let mic = open_source(SourceKind::Mic, &config.mic_track_path);
    let system = open_source(SourceKind::System, &config.system_track_path);
    let (mut mic, mut system) = match (mic, system) {
        (Err(mic_err), Err(sys_err)) => {
            let err = AudioError::NoSources {
                mic: mic_err.to_string(),
                system: sys_err.to_string(),
            };
            let _ = ready.send(Err(AudioError::NoSources {
                mic: mic_err.to_string(),
                system: sys_err.to_string(),
            }));
            return Err(err);
        }
        (mic, system) => (
            keep(mic, SourceKind::Mic, &mut status),
            keep(system, SourceKind::System, &mut status),
        ),
    };

    let mut mix = match OggOpusWriter::create(&config.mix_path, MIX_BITRATE) {
        Ok(w) => w,
        Err(err) => {
            let _ = ready.send(Err(AudioError::Io(std::io::Error::other(err.to_string()))));
            return Err(err);
        }
    };
    let _ = ready.send(Ok(status));

    let mut mic_buf = Vec::with_capacity(SAMPLE_RATE as usize);
    let mut sys_buf = Vec::with_capacity(SAMPLE_RATE as usize);
    let mut mixed = Vec::with_capacity(SAMPLE_RATE as usize);

    let mut produced: u64 = 0;
    let mut segment_base: u64 = 0;
    let mut segment_start = Instant::now();
    let mut paused = false;
    let mut last_level = Instant::now();

    loop {
        match rx.recv_timeout(TICK) {
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Cmd::Pause) if !paused => {
                paused = true;
                for s in [mic.as_mut(), system.as_mut()].into_iter().flatten() {
                    if let Err(err) = s.capture.stream.pause() {
                        tracing::warn!(%err, "could not pause stream");
                    }
                }
                on_level(Levels::default());
                continue;
            }
            Ok(Cmd::Resume) if paused => {
                paused = false;
                for s in [mic.as_mut(), system.as_mut()].into_iter().flatten() {
                    s.discard_buffered();
                    if let Err(err) = s.capture.stream.play() {
                        tracing::warn!(%err, "could not resume stream");
                    }
                }
                segment_base = produced;
                segment_start = Instant::now();
                continue;
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
        }
        if paused {
            continue;
        }

        let running = segment_start.elapsed().saturating_sub(LATENCY);
        let target = segment_base + (running.as_secs_f64() * f64::from(SAMPLE_RATE)) as u64;
        let need = usize::try_from(target.saturating_sub(produced))
            .unwrap_or(0)
            .min(SAMPLE_RATE as usize);
        if need == 0 {
            continue;
        }

        let mut levels = Levels::default();
        if let Some(s) = mic.as_mut() {
            s.check_failure(SourceKind::Mic);
            s.pull();
            s.take(need, &mut mic_buf);
            s.track.write(&mic_buf)?;
            levels.mic = mixer::meter(mixer::rms(&mic_buf));
        } else {
            mic_buf.clear();
            mic_buf.resize(need, 0.0);
        }
        if let Some(s) = system.as_mut() {
            s.check_failure(SourceKind::System);
            s.pull();
            s.take(need, &mut sys_buf);
            s.track.write(&sys_buf)?;
            levels.system = mixer::meter(mixer::rms(&sys_buf));
        } else {
            sys_buf.clear();
            sys_buf.resize(need, 0.0);
        }

        mixer::mix(&mic_buf, &sys_buf, &mut mixed);
        mix.write(&mixed)?;
        produced += need as u64;

        if last_level.elapsed() >= LEVEL_EVERY {
            last_level = Instant::now();
            on_level(levels);
        }
    }

    // Stop capturing before finalizing files.
    let mut summary = SessionSummary {
        duration_ms: produced * 1000 / u64::from(SAMPLE_RATE),
        bytes: 0,
    };
    for (kind, source) in [(SourceKind::Mic, mic), (SourceKind::System, system)] {
        if let Some(s) = source {
            drop(s.capture.stream);
            tracing::info!(
                %kind,
                dropped = s.capture.dropped.load(Ordering::Relaxed),
                underruns = s.underruns,
                "source finished"
            );
            s.track.finish()?;
        }
    }
    summary.bytes = mix.finish()?;
    on_level(Levels::default());
    tracing::info!(
        duration_ms = summary.duration_ms,
        bytes = summary.bytes,
        "recording finished"
    );
    Ok(summary)
}

fn keep(
    source: Result<Source, AudioError>,
    kind: SourceKind,
    status: &mut SourceStatus,
) -> Option<Source> {
    match source {
        Ok(s) => {
            let name = Some(s.capture.device_name.clone());
            match kind {
                SourceKind::Mic => status.mic = name,
                SourceKind::System => status.system = name,
            }
            Some(s)
        }
        Err(err) => {
            tracing::warn!(%kind, %err, "source unavailable, recording without it");
            status.warnings.push(format!("Not recording {kind}: {err}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    /// RMS of a decoded Ogg Opus file.
    fn decoded_rms(path: &Path) -> (f32, usize) {
        let mut reader = ogg::reading::PacketReader::new(File::open(path).unwrap());
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).unwrap();
        let mut pcm = vec![0f32; super::super::FRAME];
        let (mut sum, mut n) = (0f64, 0usize);
        let mut index = 0;
        while let Some(p) = reader.read_packet().unwrap() {
            index += 1;
            if index <= 2 {
                continue; // OpusHead, OpusTags
            }
            let got = decoder.decode_float(&p.data, &mut pcm, false).unwrap();
            for s in &pcm[..got] {
                sum += f64::from(*s) * f64::from(*s);
            }
            n += got;
        }
        (((sum / n.max(1) as f64).sqrt()) as f32, n)
    }

    /// Records from the real default devices. Run manually while audio plays:
    /// `cargo test records_real_devices -- --ignored --nocapture`
    /// Set TASKMIND_TEST_OUT to keep the files.
    #[test]
    #[ignore = "uses real audio devices and needs permissions"]
    fn records_real_devices() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = std::env::var("TASKMIND_TEST_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| tmp.path().to_path_buf());
        let config = SessionConfig {
            mix_path: dir.join("mix.ogg"),
            mic_track_path: dir.join("mic.ogg"),
            system_track_path: dir.join("system.ogg"),
        };
        let session = Session::start(config, |_| {}).unwrap();
        eprintln!("sources: {:?}", session.sources);
        std::thread::sleep(Duration::from_secs(6));
        let summary = session.stop().unwrap();
        eprintln!("summary: {summary:?}");
        assert!(summary.duration_ms >= 5_000, "{summary:?}");

        for name in ["mix.ogg", "mic.ogg", "system.ogg"] {
            let path = dir.join(name);
            if path.exists() {
                let (rms, samples) = decoded_rms(&path);
                let bytes = std::fs::metadata(&path).unwrap().len();
                eprintln!(
                    "{name}: {bytes} bytes, {:.2} s, rms {rms:.4} ({:.1} dBFS)",
                    samples as f32 / SAMPLE_RATE as f32,
                    20.0 * rms.max(1e-9).log10()
                );
            }
        }
    }
}
