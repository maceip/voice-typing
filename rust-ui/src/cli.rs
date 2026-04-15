use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;
use voice_typing_asr::{CurrentWiredModel, DesktopSherpaAsrService};
use voice_typing_core::AsrService;

pub fn run(args: &[String]) -> Result<()> {
    match parse_command(args)? {
        CliCommand::Mic => run_mic(),
        CliCommand::Wav(path) => run_wav(path),
        CliCommand::Help => {
            print_global_usage();
            Ok(())
        }
    }
}

enum CliCommand {
    Mic,
    Wav(PathBuf),
    Help,
}

fn parse_command(args: &[String]) -> Result<CliCommand> {
    match args {
        [] => Ok(CliCommand::Mic),
        [command] if command == "mic" => Ok(CliCommand::Mic),
        [command] if command == "help" || command == "--help" || command == "-h" => {
            Ok(CliCommand::Help)
        }
        [command] if command == "wav" => Ok(CliCommand::Wav(default_wav_path()?)),
        [command, path] if command == "wav" => Ok(CliCommand::Wav(PathBuf::from(path))),
        _ => Err(anyhow!("unsupported console arguments: {}", args.join(" "))),
    }
}

fn run_mic() -> Result<()> {
    let mut service = DesktopSherpaAsrService::new();
    service.initialize_blocking(CurrentWiredModel::MODEL_DIR)?;
    let mut results = service.subscribe_results();
    service.start_real_time_session()?;

    println!("Listening. Press Ctrl+C to stop.");
    loop {
        match results.blocking_recv() {
            Ok(result) => println!("{}", result.text),
            Err(err) => return Err(anyhow!("ASR stream ended: {err}")),
        }
    }
}

fn run_wav(path: PathBuf) -> Result<()> {
    let mut service = DesktopSherpaAsrService::new();
    service.initialize_blocking(CurrentWiredModel::MODEL_DIR)?;
    let result = service.transcribe_wav(&path)?;
    println!("{}", result.text);
    Ok(())
}

fn default_wav_path() -> Result<PathBuf> {
    let root = std::env::current_dir().context("failed to resolve repo root")?;
    let model = CurrentWiredModel::locate_from(&root)?;
    Ok(model.model_dir.join("test_wavs").join("0.wav"))
}

pub fn print_global_usage() {
    eprintln!("Usage:");
    eprintln!("  voice-typing");
    eprintln!("  voice-typing --help");
    eprintln!("  voice-typing --nogui");
    eprintln!("  voice-typing --nogui mic");
    eprintln!("  voice-typing --nogui wav [path-to-wav]");
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  default        Launch the GUI");
    eprintln!("  --nogui        Run in console mode");
    eprintln!();
    eprintln!("Console Commands:");
    eprintln!("  mic            Start microphone transcription in the terminal");
    eprintln!("  wav [path]     Transcribe a WAV file");
    eprintln!("  help           Show this help");
}
