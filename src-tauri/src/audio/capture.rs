//! Opens one input stream and pushes mono f32 samples into a lock-free ring buffer.
//! The cpal callback does no allocation, locking or logging.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use ringbuf::traits::{Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::{AudioError, platform};

/// Seconds of audio the ring buffer can hold before dropping samples.
const BUFFER_SECONDS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Mic,
    System,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SourceKind::Mic => "microphone",
            SourceKind::System => "system audio",
        })
    }
}

pub struct Capture {
    pub stream: cpal::Stream,
    pub consumer: HeapCons<f32>,
    pub sample_rate: u32,
    pub device_name: String,
    pub dropped: Arc<AtomicU64>,
    pub failed: Arc<AtomicBool>,
}

pub fn open(kind: SourceKind) -> Result<Capture, AudioError> {
    let host = cpal::default_host();
    let (device, supported) = match kind {
        SourceKind::Mic => {
            let device = host
                .default_input_device()
                .ok_or(AudioError::NoDevice(kind))?;
            let config = device.default_input_config()?;
            (device, config)
        }
        SourceKind::System => {
            let device = platform::system_device(&host)?;
            let config = platform::system_config(&device)?;
            (device, config)
        }
    };

    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| kind.to_string());
    let sample_rate = supported.sample_rate();
    let channels = usize::from(supported.channels().max(1));
    let config = cpal::StreamConfig {
        channels: supported.channels(),
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let ring = HeapRb::<f32>::new(sample_rate as usize * BUFFER_SECONDS);
    let (producer, consumer) = ring.split();
    let dropped = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicBool::new(false));

    let parts = StreamParts {
        producer,
        channels,
        dropped: dropped.clone(),
        failed: failed.clone(),
        kind,
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, parts),
        SampleFormat::I16 => build::<i16>(&device, &config, parts),
        SampleFormat::I32 => build::<i32>(&device, &config, parts),
        SampleFormat::U16 => build::<u16>(&device, &config, parts),
        SampleFormat::U8 => build::<u8>(&device, &config, parts),
        SampleFormat::F64 => build::<f64>(&device, &config, parts),
        other => Err(AudioError::Unsupported(format!(
            "sample format {other} on {kind}"
        ))),
    }?;
    stream.play()?;
    tracing::info!(%kind, device = %device_name, sample_rate, channels, "capture started");

    Ok(Capture {
        stream,
        consumer,
        sample_rate,
        device_name,
        dropped,
        failed,
    })
}

struct StreamParts {
    producer: HeapProd<f32>,
    channels: usize,
    dropped: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    kind: SourceKind,
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    parts: StreamParts,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let StreamParts {
        mut producer,
        channels,
        dropped,
        failed,
        kind,
    } = parts;
    let scale = 1.0 / channels as f32;

    let stream = device.build_input_stream::<T, _, _>(
        *config,
        move |data: &[T], _info| {
            let frames = data.len() / channels;
            let mono = data
                .chunks_exact(channels)
                .map(|frame| frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() * scale);
            let pushed = producer.push_iter(mono);
            if pushed < frames {
                dropped.fetch_add((frames - pushed) as u64, Ordering::Relaxed);
            }
        },
        move |err| {
            // Error callbacks are not the realtime path, so logging is fine here.
            tracing::error!(%kind, %err, "audio stream error");
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    Ok(stream)
}
