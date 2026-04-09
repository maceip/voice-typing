use anyhow::Context;
use daydream_core::TtsService;
use std::process::Command;

pub struct DesktopTtsService;

impl TtsService for DesktopTtsService {
    fn yell(&self, message: &str) -> anyhow::Result<()> {
        if cfg!(target_os = "windows") {
            let escaped = message.replace('\'', "''");
            Command::new("powershell")
                .args([
                    "-Command",
                    &format!(
                        "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak('{escaped}')"
                    ),
                ])
                .status()
                .context("failed to start Windows speech synthesis")?;
        } else if cfg!(target_os = "macos") {
            Command::new("say")
                .arg(message)
                .status()
                .context("failed to start macOS speech synthesis")?;
        } else {
            Command::new("espeak")
                .arg(message)
                .status()
                .context("failed to start Linux speech synthesis")?;
        }

        Ok(())
    }
}
