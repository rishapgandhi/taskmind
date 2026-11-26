//! Mixing and level metering.

/// Mix mic and system audio into `out` (all three the same length), with hard clipping.
pub fn mix(mic: &[f32], system: &[f32], out: &mut Vec<f32>) {
    out.clear();
    out.extend(
        mic.iter()
            .zip(system)
            .map(|(a, b)| (a + b).clamp(-1.0, 1.0)),
    );
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Map RMS to a 0..1 meter value on a -60..0 dBFS scale.
pub fn meter(rms: f32) -> f32 {
    if rms <= 1e-6 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_sums_and_clips() {
        let mut out = Vec::new();
        mix(&[0.2, 0.8, -0.9], &[0.1, 0.5, -0.5], &mut out);
        assert!((out[0] - 0.3).abs() < 1e-6);
        assert_eq!(out[1], 1.0);
        assert_eq!(out[2], -1.0);
    }

    #[test]
    fn meter_scale() {
        assert_eq!(meter(0.0), 0.0);
        assert!((meter(1.0) - 1.0).abs() < 1e-6);
        assert!((meter(0.001) - 0.0).abs() < 1e-3); // -60 dBFS
        let half = meter(10f32.powf(-30.0 / 20.0)); // -30 dBFS
        assert!((half - 0.5).abs() < 1e-3);
    }

    #[test]
    fn rms_of_square_wave() {
        assert!((rms(&[0.5, -0.5, 0.5, -0.5]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }
}
