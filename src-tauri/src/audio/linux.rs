//! System audio on Linux: record the monitor source of the default sink.
//! Works with PulseAudio and with PipeWire through pipewire-pulse. cpal's PulseAudio host is
//! pure Rust, so users need no extra libraries. v1 picked the first `.monitor` it found,
//! which could be the wrong device. We use the default sink's monitor instead.

use cpal::traits::{DeviceTrait, HostTrait};

use super::{AudioError, SourceKind};

pub fn system_device(host: &cpal::Host) -> Result<cpal::Device, AudioError> {
    let sink = host
        .default_output_device()
        .ok_or(AudioError::NoDevice(SourceKind::System))?;
    let sink_id = sink.id()?;
    let monitor = format!("{}.monitor", sink_id.id());
    host.input_devices()?
        .find(|d| d.id().map(|id| id.id() == monitor).unwrap_or(false))
        .ok_or_else(|| {
            AudioError::Unsupported(format!(
                "no monitor source '{monitor}' (is PulseAudio or PipeWire running?)"
            ))
        })
}

pub fn system_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, AudioError> {
    Ok(device.default_input_config()?)
}
