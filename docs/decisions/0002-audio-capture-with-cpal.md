# ADR 0002: Capture all audio through cpal

Date: 2026-11-27
Status: Accepted

## Context

TaskMind needs the microphone and system audio (other meeting participants) on macOS, Windows and Linux, with no extra software for users.

## Decision

Use cpal 0.18 for every source on every OS:

- macOS: cpal builds an input stream on the default output device using a Core Audio process tap (macOS 14.6+).
- Windows: cpal uses WASAPI loopback on the default output device.
- Linux: cpal's PulseAudio host (a pure-Rust client, no libpulse needed) records the default sink's `.monitor` source. Works on PipeWire through pipewire-pulse.

Encode with libopus (statically built by the `opus` crate) into Ogg pages written with the `ogg` crate.

## Consequences

- One capture code path instead of three native implementations. OS differences are limited to `audio/loopback.rs` and `audio/linux.rs`.
- macOS 13.0 to 14.5 gets mic-only recording until a ScreenCaptureKit fallback is added.
- On macOS, cpal records a device's own input if the output device also has inputs (some headsets). We refuse system capture for those devices for now. Fixing it needs our own Core Audio tap code.
- Building needs `cmake` (for libopus) on developer machines and CI. Users need nothing.
