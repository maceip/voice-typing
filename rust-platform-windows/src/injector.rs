use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotPrefixResult {
    Consumed,
    CorrectionStarted { wrong_word: String },
    NotHotPrefix,
}

#[derive(Debug, Default)]
pub struct TextInjector {
    pending_correction: Option<String>,
}

impl TextInjector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_correction(&self) -> Option<&str> {
        self.pending_correction.as_deref()
    }

    pub fn clear_pending_correction(&mut self) {
        self.pending_correction = None;
    }

    pub fn inject_text(&mut self, text: &str) -> HotPrefixResult {
        if text.trim().is_empty() {
            return HotPrefixResult::NotHotPrefix;
        }

        let trimmed = text.trim();
        let lower = trimmed.trim_end_matches('.').to_ascii_lowercase();

        if matches!(lower.as_str(), "x enter" | "xenter" | "ex enter") {
            return HotPrefixResult::Consumed;
        }

        if let Some(wrong_word) = parse_fix_command(&lower) {
            self.pending_correction = Some(wrong_word.clone());
            return HotPrefixResult::CorrectionStarted { wrong_word };
        }

        if self.pending_correction.is_some() && !parse_spelling(trimmed).is_empty() {
            self.pending_correction = None;
            return HotPrefixResult::Consumed;
        }

        let commands = [
            "press enter",
            "new line",
            "next line",
            "go up",
            "go down",
            "go left",
            "go right",
            "press tab",
            "press escape",
            "press backspace",
            "scratch that",
            "undo that",
            "select all",
            "right click",
            "click",
        ];

        if commands.contains(&lower.as_str()) {
            return HotPrefixResult::Consumed;
        }

        HotPrefixResult::NotHotPrefix
    }

    pub fn send_to_focused_window(&mut self, text: &str) -> Result<HotPrefixResult> {
        let trimmed = text.trim();
        let lower = trimmed.trim_end_matches('.').to_ascii_lowercase();
        let result = self.inject_text(text);

        if is_enter_command(&lower) {
            send_enter_key()?;
            return Ok(HotPrefixResult::Consumed);
        }

        if matches!(result, HotPrefixResult::NotHotPrefix) {
            send_unicode_text(&(text.to_owned() + " "))?;
        }

        Ok(result)
    }
}

fn parse_fix_command(lower: &str) -> Option<String> {
    let prefixes = [
        "xfix ",
        "x fix ",
        "xcorrect ",
        "x correct ",
        "ex fix ",
        "ex correct ",
    ];
    prefixes
        .iter()
        .find_map(|prefix| lower.strip_prefix(prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_enter_command(lower: &str) -> bool {
    matches!(
        lower,
        "press enter" | "new line" | "next line" | "x enter" | "xenter" | "ex enter" | "bang bang"
    )
}

pub fn parse_spelling(text: &str) -> String {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if !parts.is_empty()
        && parts
            .iter()
            .all(|part| part.len() == 1 && part.chars().all(char::is_alphabetic))
    {
        parts.join("")
    } else {
        text.trim().to_owned()
    }
}

#[cfg(target_os = "windows")]
fn send_unicode_text(text: &str) -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
        VIRTUAL_KEY,
    };

    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);

    for unit in text.encode_utf16() {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        anyhow::bail!("SendInput typed only {sent} of {} events", inputs.len());
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn send_enter_key() -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
        VK_RETURN,
    };

    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_RETURN.0),
                    wScan: 0,
                    dwFlags: Default::default(),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_RETURN.0),
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        anyhow::bail!(
            "SendInput typed only {sent} of {} enter events",
            inputs.len()
        );
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn send_unicode_text(_text: &str) -> Result<()> {
    anyhow::bail!("text injection is only implemented on Windows")
}

#[cfg(not(target_os = "windows"))]
fn send_enter_key() -> Result<()> {
    anyhow::bail!("text injection is only implemented on Windows")
}
