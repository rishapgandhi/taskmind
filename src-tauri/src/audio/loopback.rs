//! System audio on macOS and Windows: record the default output device.
//! cpal turns an input stream on an output device into loopback capture
//! (Core Audio process tap on macOS 14.6+, WASAPI loopback on Windows).

use cpal::traits::{DeviceTrait, HostTrait};

use super::{AudioError, SourceKind};

pub fn system_device(host: &cpal::Host) -> Result<cpal::Device, AudioError> {
    let device = host
        .default_output_device()
        .ok_or(AudioError::NoDevice(SourceKind::System))?;

    // On macOS, cpal records the device's own input if it has one (some USB/Bluetooth headsets
    // expose input and output on one device). That would record the headset mic twice, so refuse.
    #[cfg(target_os = "macos")]
    if device.supports_input() {
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_default();
        return Err(AudioError::Unsupported(format!(
            "system audio capture is not supported yet for output device '{name}' (it is also an input device)"
        )));
    }

    Ok(device)
}

pub fn system_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, AudioError> {
    Ok(device.default_output_config()?)
}
