# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-04-22

### Added

- Automatic Moonshine model and Silero VAD download with bundled-asset fallback for the desktop app.
- Windows MSI packaging via WiX, including a Start Menu shortcut and a local `scripts/build-msi.ps1` helper.
- New README media assets: a demo GIF plus compact and full settings screenshots.
- Publish-ready metadata and README files across the workspace crates.

### Changed

- Refined the acrylic widget shell and refreshed app, tray, and extension icons.
- Expanded Windows text injection support with better target detection and key-chord command support.
- Polished the settings surface for behavior, model source, and window material choices.
- Root `cargo build` now defaults to the `voice-typing` desktop app instead of the whole workspace.
- Browser bridge dependencies now build only when `--features extensions` is enabled.

### Fixed

- Single-instance startup now reports an already-running app when the bridge port is already occupied.
- The MSI build script now honors the selected cargo configuration instead of always forcing `--release`.
