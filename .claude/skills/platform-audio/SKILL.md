---
name: platform-audio
description: Use when implementing, changing or debugging audio capture, mixing, echo cancellation or Opus encoding in TaskMind v2 (src-tauri/src/audio/, recorder.rs), or when a recording is silent, one-sided, echoing, too large or corrupt on macOS, Windows or Linux.
---

# Platform audio in TaskMind v2

Read `docs/ARCHITECTURE.md` (Audio pipeline, Per-OS strategy) first.

## How it is built (see ADR 0002)

- All capture goes through cpal 0.18. `audio/capture.rs` opens a stream and pushes mono f32 into a lock-free `ringbuf`. The callback must not allocate, lock or log.
- OS differences live only in `audio/loopback.rs` (macOS + Windows: default output device, loopback) and `audio/linux.rs` (default sink's `.monitor` source).
- `audio/session.rs` is the recorder thread: owns the streams, ticks every 20 ms, aligns sources to the wall clock with a 150 ms buffer, pads silence on underrun, drops backlog over 250 ms, writes the mixed file and two tracks.
- `audio/encoder.rs` writes Ogg Opus (RFC 7845). Granule positions count real samples, so the final padded frame is trimmed by players.
- `recorder/mod.rs` creates the meeting row, starts/stops the session and emits `recorder://state`, `recorder://level` (10 Hz), `recorder://error`, `meetings://changed`.

## Per OS notes

**macOS**
- System audio needs macOS 14.6+ and the "System Audio Recording" permission (`NSAudioCaptureUsageDescription` in `src-tauri/Info.plist`). Mic needs `NSMicrophoneUsageDescription`.
- In `pnpm tauri dev`, macOS attributes permissions to the terminal app that launched it.
- If the default output device also has inputs, cpal would record that device's mic. `loopback.rs` refuses these devices. Fix needs a direct Core Audio tap.

**Windows**
- Loopback sends no data during silence. The session pads with zeros, so this is expected (counted as underruns in logs).

**Linux**
- Needs a running PulseAudio or PipeWire (pipewire-pulse). With plain ALSA there is no monitor source and recording is mic-only with a warning.
- Never load PulseAudio modules (v1 did).

## Debug checklist

1. Silent system track: check permission (macOS), wrong device (Linux default sink, Windows default endpoint), or app audio routed to a different device.
2. Only one side audible: mic and system both started? Check frame counters per source in `tracing` logs.
3. Echo / double words: AEC reference misaligned. Compare timestamps of the two tracks, check AEC delay estimate.
4. Drift over long calls: resample by measured rate, not nominal rate. Log the drift every minute.
5. Large file: confirm Opus bitrate and mono. Expect about 14 MB per hour.
6. Corrupt file after crash: verify page flush interval.

## Testing

- Unit tests cover the resampler, mixer, encoder (decodes the file back) and recorder state machine.
- Hardware test (skipped by default): play audio, then run `cargo test --manifest-path src-tauri/Cargo.toml records_real_devices -- --ignored --nocapture`. It prints size and loudness of each file. Silence on the system track while audio plays means a permission or device problem. Set `TASKMIND_TEST_OUT=/some/dir` to keep the files.
- Manual test matrix per release: 60-minute call in Meet + Zoom on each OS, headphones and speakers, device change mid-call. Record results in the PR.
