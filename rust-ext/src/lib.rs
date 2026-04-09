//! Embeds Daydream browser extensions and writes them to disk.
//!
//! `cargo build` with the `extensions` feature produces:
//!   - `extensions/chrome/`   unpacked directory (load in chrome://extensions)
//!   - `extensions/safari/`   unpacked directory (convert with xcrun)
//!   - `extensions/daydream-chrome.zip`
//!   - `extensions/daydream-safari.zip`

use std::io;
use std::path::Path;

// ── Embedded zip archives (built by build.rs) ───────────────────────────

pub const CHROME_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/chrome.zip"));
pub const SAFARI_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/safari.zip"));

// ── Embedded source files for unpacked loading ──────────────────────────

pub mod chrome {
    pub const MANIFEST: &str = include_str!("../chrome/manifest.json");
    pub const BACKGROUND: &str = include_str!("../chrome/background.js");
    pub const CONTENT_JS: &str = include_str!("../chrome/content.js");
    pub const CONTENT_CSS: &str = include_str!("../chrome/content.css");
    pub const ICON_16: &[u8] = include_bytes!("../chrome/icons/icon16.png");
    pub const ICON_32: &[u8] = include_bytes!("../chrome/icons/icon32.png");
    pub const ICON_48: &[u8] = include_bytes!("../chrome/icons/icon48.png");
    pub const ICON_128: &[u8] = include_bytes!("../chrome/icons/icon128.png");
}

pub mod safari {
    pub const MANIFEST: &str = include_str!("../safari/manifest.json");
    pub const BACKGROUND: &str = include_str!("../safari/background.js");
    pub const CONTENT_JS: &str = include_str!("../safari/content.js");
    pub const CONTENT_CSS: &str = include_str!("../safari/content.css");
    pub const INFO_PLIST: &str = include_str!("../safari/Info.plist");
    pub const ICON_16: &[u8] = include_bytes!("../safari/icons/icon16.png");
    pub const ICON_32: &[u8] = include_bytes!("../safari/icons/icon32.png");
    pub const ICON_48: &[u8] = include_bytes!("../safari/icons/icon48.png");
    pub const ICON_128: &[u8] = include_bytes!("../safari/icons/icon128.png");
}

/// Write all extension artifacts under `dir`.
pub fn write(dir: &Path) -> io::Result<()> {
    // ── Chrome unpacked ─────────────────────────────────────────────
    let chrome_dir = dir.join("chrome");
    let chrome_icons = chrome_dir.join("icons");
    std::fs::create_dir_all(&chrome_icons)?;
    std::fs::write(chrome_dir.join("manifest.json"), chrome::MANIFEST)?;
    std::fs::write(chrome_dir.join("background.js"), chrome::BACKGROUND)?;
    std::fs::write(chrome_dir.join("content.js"), chrome::CONTENT_JS)?;
    std::fs::write(chrome_dir.join("content.css"), chrome::CONTENT_CSS)?;
    std::fs::write(chrome_icons.join("icon16.png"), chrome::ICON_16)?;
    std::fs::write(chrome_icons.join("icon32.png"), chrome::ICON_32)?;
    std::fs::write(chrome_icons.join("icon48.png"), chrome::ICON_48)?;
    std::fs::write(chrome_icons.join("icon128.png"), chrome::ICON_128)?;

    // ── Safari unpacked ─────────────────────────────────────────────
    let safari_dir = dir.join("safari");
    let safari_icons = safari_dir.join("icons");
    std::fs::create_dir_all(&safari_icons)?;
    std::fs::write(safari_dir.join("manifest.json"), safari::MANIFEST)?;
    std::fs::write(safari_dir.join("background.js"), safari::BACKGROUND)?;
    std::fs::write(safari_dir.join("content.js"), safari::CONTENT_JS)?;
    std::fs::write(safari_dir.join("content.css"), safari::CONTENT_CSS)?;
    std::fs::write(safari_dir.join("Info.plist"), safari::INFO_PLIST)?;
    std::fs::write(safari_icons.join("icon16.png"), safari::ICON_16)?;
    std::fs::write(safari_icons.join("icon32.png"), safari::ICON_32)?;
    std::fs::write(safari_icons.join("icon48.png"), safari::ICON_48)?;
    std::fs::write(safari_icons.join("icon128.png"), safari::ICON_128)?;

    // ── Zip archives ────────────────────────────────────────────────
    std::fs::write(dir.join("daydream-chrome.zip"), CHROME_ZIP)?;
    std::fs::write(dir.join("daydream-safari.zip"), SAFARI_ZIP)?;

    Ok(())
}
