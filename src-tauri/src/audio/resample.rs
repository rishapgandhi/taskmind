//! Streaming linear resampler, good enough for speech at meeting quality.
//! It keeps state across chunks so there are no clicks at chunk borders.
//! TODO(M2): switch to a band-limited resampler (rubato) before downsampling to 16 kHz for Whisper.

pub struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            step: f64::from(from_rate) / f64::from(to_rate),
            pos: 0.0,
            last: 0.0,
        }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.step - 1.0).abs() < f64::EPSILON
    }

    /// Append resampled `input` to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        if self.is_passthrough() {
            out.extend_from_slice(input);
            return;
        }
        let n = input.len();
        let last = self.last;
        // Virtual signal: index -1 is the previous chunk's last sample.
        let at = |i: isize| if i < 0 { last } else { input[i as usize] };
        while self.pos < (n - 1) as f64 {
            let i = self.pos.floor();
            let frac = (self.pos - i) as f32;
            let a = at(i as isize);
            let b = at(i as isize + 1);
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        self.pos -= n as f64;
        self.last = input[n - 1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(from: u32, to: u32, total: usize, chunk: usize) -> Vec<f32> {
        let mut r = Resampler::new(from, to);
        let input: Vec<f32> = (0..total).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut out = Vec::new();
        for c in input.chunks(chunk) {
            r.process(c, &mut out);
        }
        out
    }

    #[test]
    fn output_length_matches_ratio() {
        for (from, to) in [
            (44_100, 48_000),
            (16_000, 48_000),
            (96_000, 48_000),
            (24_000, 48_000),
        ] {
            let out = run(from, to, from as usize * 2, 441);
            let expected = 2 * to as usize;
            let diff = out.len().abs_diff(expected);
            assert!(
                diff <= 2,
                "{from}->{to}: got {} expected {expected}",
                out.len()
            );
        }
    }

    #[test]
    fn passthrough_is_exact() {
        let mut r = Resampler::new(48_000, 48_000);
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn constant_signal_stays_constant_across_chunks() {
        let mut r = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        r.process(&[0.5; 100], &mut out);
        let first = out.len();
        r.process(&[0.5; 100], &mut out);
        // Skip the very first sample, which ramps up from the initial zero state.
        assert!(out[1..].iter().all(|&s| (s - 0.5).abs() < 1e-6));
        assert!(out.len() > first);
    }
}
