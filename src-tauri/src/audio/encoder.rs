//! Ogg Opus file writer (RFC 7845), mono, 48 kHz.
//! Pages are written straight to the file about once per second, so a crash keeps almost all audio.

use std::fs::File;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder};

use super::{AudioError, FRAME, SAMPLE_RATE};

/// Max bytes of one Opus packet (RFC 6716 recommends 4000).
const MAX_PACKET: usize = 4000;
/// Packets per Ogg page: 50 x 20 ms = 1 second.
const PACKETS_PER_PAGE: u32 = 50;
const VENDOR: &str = concat!("TaskMind ", env!("CARGO_PKG_VERSION"));

pub struct OggOpusWriter {
    encoder: Encoder,
    writer: PacketWriter<'static, File>,
    serial: u32,
    pre_skip: u64,
    /// Real (unpadded) samples received.
    samples_in: u64,
    /// Samples already encoded into packets.
    samples_encoded: u64,
    pending: Vec<f32>,
    packets_in_page: u32,
    out: Vec<u8>,
}

impl OggOpusWriter {
    pub fn create(path: &Path, bitrate: i32) -> Result<Self, AudioError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut encoder = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)?;
        encoder.set_bitrate(Bitrate::Bits(bitrate))?;
        let pre_skip = u64::try_from(encoder.get_lookahead()?).unwrap_or(312);

        let serial = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
            .unwrap_or(0x5441_534b);
        let mut writer = PacketWriter::new(File::create(path)?);
        writer.write_packet(
            opus_head(pre_skip as u16),
            serial,
            PacketWriteEndInfo::EndPage,
            0,
        )?;
        writer.write_packet(opus_tags(), serial, PacketWriteEndInfo::EndPage, 0)?;

        Ok(Self {
            encoder,
            writer,
            serial,
            pre_skip,
            samples_in: 0,
            samples_encoded: 0,
            pending: Vec::with_capacity(FRAME * 2),
            packets_in_page: 0,
            out: vec![0; MAX_PACKET],
        })
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<(), AudioError> {
        self.samples_in += samples.len() as u64;
        self.pending.extend_from_slice(samples);
        let mut offset = 0;
        while self.pending.len() - offset >= FRAME {
            let frame: Vec<f32> = self.pending[offset..offset + FRAME].to_vec();
            self.packets_in_page += 1;
            let end = if self.packets_in_page >= PACKETS_PER_PAGE {
                self.packets_in_page = 0;
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            self.encode(&frame, end)?;
            offset += FRAME;
        }
        self.pending.drain(..offset);
        Ok(())
    }

    /// Encode what is left (padded with silence) and close the stream. Returns the file size.
    pub fn finish(mut self) -> Result<u64, AudioError> {
        let mut frame = std::mem::take(&mut self.pending);
        frame.resize(FRAME, 0.0);
        self.encode(&frame, PacketWriteEndInfo::EndStream)?;
        let file = self.writer.into_inner();
        file.sync_all()?;
        Ok(file.metadata()?.len())
    }

    fn encode(&mut self, frame: &[f32], end: PacketWriteEndInfo) -> Result<(), AudioError> {
        let len = self.encoder.encode_float(frame, &mut self.out)?;
        self.samples_encoded += FRAME as u64;
        // Granule position counts real samples only, so players trim the final padding.
        let granule = self.pre_skip + self.samples_encoded.min(self.samples_in);
        self.writer
            .write_packet(self.out[..len].to_vec(), self.serial, end, granule)?;
        Ok(())
    }
}

fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut h = Vec::with_capacity(19);
    h.extend_from_slice(b"OpusHead");
    h.push(1); // version
    h.push(1); // channels
    h.extend_from_slice(&pre_skip.to_le_bytes());
    h.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    h.extend_from_slice(&0i16.to_le_bytes()); // output gain
    h.push(0); // channel mapping family
    h
}

fn opus_tags() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(b"OpusTags");
    t.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    t.extend_from_slice(VENDOR.as_bytes());
    t.extend_from_slice(&0u32.to_le_bytes()); // no user comments
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogg::reading::PacketReader;

    fn sine(samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn writes_valid_ogg_opus() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("t.ogg");
        let mut w = OggOpusWriter::create(&path, 32_000).unwrap();
        let audio = sine(SAMPLE_RATE as usize + 123); // 1 s + a partial frame
        for chunk in audio.chunks(700) {
            w.write(chunk).unwrap();
        }
        let pre_skip = w.pre_skip;
        let bytes = w.finish().unwrap();
        assert!(bytes > 1000);

        let mut reader = PacketReader::new(File::open(&path).unwrap());
        let head = reader.read_packet_expected().unwrap();
        assert!(head.data.starts_with(b"OpusHead"));
        assert!(head.first_in_stream());
        let tags = reader.read_packet_expected().unwrap();
        assert!(tags.data.starts_with(b"OpusTags"));

        let mut decoder = opus::Decoder::new(SAMPLE_RATE, Channels::Mono).unwrap();
        let mut pcm = vec![0f32; FRAME];
        let mut packets = 0;
        let mut last = None;
        while let Some(p) = reader.read_packet().unwrap() {
            assert_eq!(
                decoder.decode_float(&p.data, &mut pcm, false).unwrap(),
                FRAME
            );
            packets += 1;
            last = Some(p);
        }
        let last = last.unwrap();
        assert!(last.last_in_stream());
        assert_eq!(last.absgp_page(), pre_skip + audio.len() as u64);
        assert_eq!(packets, audio.len().div_ceil(FRAME));
    }

    #[test]
    fn one_hour_size_estimate_is_small() {
        // 10 s of tone at 32 kbps should be about 40 KB, so an hour is about 14 MB.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.ogg");
        let mut w = OggOpusWriter::create(&path, 32_000).unwrap();
        w.write(&sine(SAMPLE_RATE as usize * 10)).unwrap();
        let bytes = w.finish().unwrap();
        assert!(bytes < 60_000, "10 s took {bytes} bytes");
    }

    #[test]
    fn empty_recording_is_still_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.ogg");
        let w = OggOpusWriter::create(&path, 32_000).unwrap();
        w.finish().unwrap();
        let mut reader = PacketReader::new(File::open(&path).unwrap());
        let mut count = 0;
        while reader.read_packet().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
    }
}
