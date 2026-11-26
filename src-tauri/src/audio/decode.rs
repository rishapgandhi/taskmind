//! Read our Ogg Opus files back as PCM, and downsample 48 kHz to 16 kHz for Whisper.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ogg::reading::PacketReader;
use opus::{Channels, Decoder};

#[cfg(test)]
use super::FRAME;
use super::{AudioError, SAMPLE_RATE};

/// Max samples one Opus packet can decode to (120 ms at 48 kHz).
const MAX_PACKET_SAMPLES: usize = 5760;

pub struct OpusFileReader {
    reader: PacketReader<BufReader<File>>,
    decoder: Decoder,
    skip: usize,
    pcm: Vec<f32>,
    done: bool,
}

impl OpusFileReader {
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let mut reader = PacketReader::new(BufReader::new(File::open(path)?));
        let head =
            read(&mut reader)?.ok_or_else(|| AudioError::Encoder("empty audio file".into()))?;
        if !head.starts_with(b"OpusHead") || head.len() < 19 {
            return Err(AudioError::Encoder("not an Ogg Opus file".into()));
        }
        if head[9] != 1 {
            return Err(AudioError::Encoder(
                "only mono recordings are supported".into(),
            ));
        }
        let skip = usize::from(u16::from_le_bytes([head[10], head[11]]));
        read(&mut reader)?; // OpusTags
        Ok(Self {
            reader,
            decoder: Decoder::new(SAMPLE_RATE, Channels::Mono)?,
            skip,
            pcm: vec![0.0; MAX_PACKET_SAMPLES],
            done: false,
        })
    }

    /// Append up to roughly `max` samples (48 kHz mono) to `out`. Returns false at end of file.
    /// A truncated file (crash during recording) ends cleanly at the last complete page.
    pub fn read_into(&mut self, out: &mut Vec<f32>, max: usize) -> Result<bool, AudioError> {
        let target = out.len() + max;
        while out.len() < target {
            if self.done {
                return Ok(false);
            }
            let packet = match read(&mut self.reader) {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.done = true;
                    continue;
                }
                Err(err) => {
                    tracing::warn!(%err, "audio file ends early, using what was read");
                    self.done = true;
                    continue;
                }
            };
            let n = self.decoder.decode_float(&packet, &mut self.pcm, false)?;
            let start = self.skip.min(n);
            self.skip -= start;
            out.extend_from_slice(&self.pcm[start..n]);
        }
        Ok(true)
    }
}

fn read(reader: &mut PacketReader<BufReader<File>>) -> Result<Option<Vec<u8>>, AudioError> {
    reader
        .read_packet()
        .map(|p| p.map(|p| p.data))
        .map_err(|e| AudioError::Encoder(e.to_string()))
}

/// RMS per block of `block` samples for a whole file. Used to tell "You" from "Others".
pub fn envelope(path: &Path, block: usize) -> Result<Vec<f32>, AudioError> {
    let mut reader = OpusFileReader::open(path)?;
    let mut buf = Vec::with_capacity(block * 50);
    let mut env = Vec::new();
    let mut more = true;
    while more {
        more = reader.read_into(&mut buf, block * 50)?;
        let full = buf.len() / block * block;
        let take = if more { full } else { buf.len() };
        for chunk in buf[..take].chunks(block) {
            env.push(super::mixer::rms(chunk));
        }
        buf.drain(..take);
    }
    Ok(env)
}

/// 48 kHz -> 16 kHz: windowed-sinc low-pass (7.2 kHz cutoff) then keep every 3rd sample.
pub struct Decimator3 {
    taps: Vec<f32>,
    history: Vec<f32>,
    phase: usize,
}

impl Default for Decimator3 {
    fn default() -> Self {
        const N: usize = 63;
        let cutoff = 7_200.0 / SAMPLE_RATE as f32; // normalized to sample rate
        let mid = (N - 1) as f32 / 2.0;
        let mut taps: Vec<f32> = (0..N)
            .map(|i| {
                let x = i as f32 - mid;
                let sinc = if x == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * std::f32::consts::PI * cutoff * x).sin() / (std::f32::consts::PI * x)
                };
                let window =
                    0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (N - 1) as f32).cos();
                sinc * window
            })
            .collect();
        let sum: f32 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= sum);
        Self {
            history: vec![0.0; N - 1],
            taps,
            phase: 0,
        }
    }
}

impl Decimator3 {
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = self.taps.len();
        self.history.extend_from_slice(input);
        let mut i = self.phase;
        while i + n <= self.history.len() {
            let window = &self.history[i..i + n];
            out.push(window.iter().zip(&self.taps).map(|(s, t)| s * t).sum());
            i += 3;
        }
        let consumed = i.min(self.history.len() - (n - 1));
        self.phase = i - consumed;
        self.history.drain(..consumed);
    }
}

/// Whole file as 16 kHz mono. Only for short files and tests. Long files are streamed.
#[cfg(test)]
pub fn read_16k(path: &Path) -> Result<Vec<f32>, AudioError> {
    let mut reader = OpusFileReader::open(path)?;
    let mut dec = Decimator3::default();
    let mut buf = Vec::new();
    let mut out = Vec::new();
    while reader.read_into(&mut buf, FRAME * 50)? {
        dec.process(&buf, &mut out);
        buf.clear();
    }
    dec.process(&buf, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::encoder::OggOpusWriter;

    fn tone(freq: f32, seconds: f32, amp: f32) -> Vec<f32> {
        (0..(SAMPLE_RATE as f32 * seconds) as usize)
            .map(|i| (i as f32 * freq * std::f32::consts::TAU / SAMPLE_RATE as f32).sin() * amp)
            .collect()
    }

    #[test]
    fn round_trip_keeps_length_and_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.ogg");
        let mut w = OggOpusWriter::create(&path, 32_000).unwrap();
        w.write(&tone(440.0, 2.0, 0.5)).unwrap();
        w.finish().unwrap();

        let audio = read_16k(&path).unwrap();
        let secs = audio.len() as f32 / 16_000.0;
        assert!((secs - 2.0).abs() < 0.05, "got {secs} s");
        let rms = crate::audio::mixer::rms(&audio[1600..audio.len() - 1600]);
        assert!((rms - 0.354).abs() < 0.05, "rms {rms}"); // 0.5 / sqrt(2)
    }

    #[test]
    fn decimator_blocks_aliasing_tone() {
        // 12 kHz cannot exist at 16 kHz; it must be filtered out, not folded to 4 kHz.
        let mut d = Decimator3::default();
        let mut out = Vec::new();
        for chunk in tone(12_000.0, 1.0, 0.5).chunks(1000) {
            d.process(chunk, &mut out);
        }
        assert!((out.len() as i64 - 16_000).abs() <= 25);
        assert!(crate::audio::mixer::rms(&out[100..]) < 0.01);
        // A 1 kHz tone passes.
        let mut d = Decimator3::default();
        let mut out = Vec::new();
        d.process(&tone(1_000.0, 1.0, 0.5), &mut out);
        assert!((crate::audio::mixer::rms(&out[100..]) - 0.354).abs() < 0.02);
    }

    #[test]
    fn envelope_has_one_value_per_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.ogg");
        let mut w = OggOpusWriter::create(&path, 24_000).unwrap();
        let mut audio = tone(440.0, 1.0, 0.5);
        audio.extend(vec![0.0; SAMPLE_RATE as usize]);
        w.write(&audio).unwrap();
        w.finish().unwrap();
        let env = envelope(&path, 4800).unwrap();
        assert!((env.len() as i64 - 20).abs() <= 1, "{}", env.len());
        assert!(env[3] > 0.2);
        assert!(env[17] < 0.01);
    }
}
