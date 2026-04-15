#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod bridge;
mod cli;
mod gui;
mod icons;
mod model_download;

use anyhow::{Result, anyhow};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();

    #[cfg(feature = "extensions")]
    write_extensions();

    if args
        .iter()
        .any(|arg| arg == "--help" || arg == "-h" || arg == "help")
    {
        cli::print_global_usage();
        return Ok(());
    }

    if let Some(index) = args.iter().position(|arg| arg == "--nogui") {
        args.remove(index);
        cli::run(&args)
    } else {
        bridge::spawn()
            .map_err(|err| anyhow!("another voice-typing instance is already running: {err}"))?;
        gui::run().map_err(|err| anyhow!(err.to_string()))
    }
}

#[cfg(feature = "extensions")]
fn write_extensions() {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("extensions")))
        .unwrap_or_else(|| std::path::PathBuf::from("extensions"));

    if let Err(e) = voice_typing_ext::write(&dir) {
        eprintln!("[ext] write failed: {e}");
    } else {
        eprintln!("[ext] {}", dir.display());
    }
}
