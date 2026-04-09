# Rust ASR

This crate is the Rust ASR library for Daydream.

## What Exists

- `DesktopSherpaAsrService` implementing the shared `AsrService` trait
- microphone capture with `cpal`
- resampling/downmix to 16 kHz mono
- Silero VAD segmentation
- in-memory microphone streaming into VAD and recognizer

## Commands

The runtime entrypoint lives in the single `daydream` binary:

```powershell
cargo run -p daydream
cargo run -p daydream -- --nogui
cargo run -p daydream -- --nogui wav [path-to-wav]
```

The standalone extracted repo owns its own model assets under `assets/models`.
