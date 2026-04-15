# Voice-Typing Rust App

This directory contains the single Rust binary for the standalone Rust-only voice-typing repo.

## What It Proves

- The app launches the GUI by default.
- Passing `--nogui` runs the console mode against the same ASR service.
- There is only one Rust binary target in the workspace now.

## Run

```powershell
cargo run
cargo run -- --nogui
cargo run -- --nogui wav [path-to-wav]
```

## Current Scope

- Compact always-on-top microphone shell built with `iced`
- Shared Rust ASR backend wired into the GUI and console entrypoints
- Windows text injection path through the platform bridge crate

## Notes

This extracted repo is intended to run independently of the Kotlin/Gradle tree.
