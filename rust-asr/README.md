# Rust ASR

This crate is the Rust ASR library for voice-typing.

## What Exists

- `DesktopSherpaAsrService` implementing the shared `AsrService` trait
- microphone capture with `cpal`
- resampling/downmix to 16 kHz mono
- Silero VAD segmentation
- in-memory microphone streaming into VAD and recognizer

## Commands

The runtime entrypoint lives in the single desktop binary in this workspace:

```powershell
cargo run
cargo run -- --nogui
cargo run -- --nogui wav [path-to-wav]
```

The standalone extracted repo owns its own model assets under `assets/models`.
