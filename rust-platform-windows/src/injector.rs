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
        let lower = normalize_hot_command(trimmed);

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
        let lower = normalize_hot_command(trimmed);
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

    pub fn has_text_entry_target(&self) -> bool {
        has_text_entry_target()
    }

    pub fn send_key_chord_to_focused_window(&self, chord: &str) -> Result<()> {
        send_key_chord(chord)
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

fn normalize_hot_command(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphabetic() || ch.is_ascii_whitespace() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
fn has_text_entry_target() -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
    };

    let foreground = unsafe { GetForegroundWindow() };
    if foreground == HWND(std::ptr::null_mut()) {
        return false;
    }

    let thread_id = unsafe { GetWindowThreadProcessId(foreground, None) };
    if thread_id == 0 {
        return false;
    }

    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetGUIThreadInfo(thread_id, &mut info) }.is_err() {
        return false;
    }

    let focus = if info.hwndFocus != HWND(std::ptr::null_mut()) {
        info.hwndFocus
    } else {
        info.hwndCaret
    };

    if focus == HWND(std::ptr::null_mut()) {
        return false;
    }

    let current_pid = std::process::id();
    let foreground_pid = unsafe { GetWindowThreadProcessId(foreground, None) };
    if foreground_pid == current_pid {
        return false;
    }

    let class_name = window_class_name(focus);
    if matches!(
        class_name.as_str(),
        "Edit"
            | "RichEdit20W"
            | "RichEdit50W"
            | "RICHEDIT50W"
            | "Chrome_RenderWidgetHostHWND"
            | "MozillaWindowClass"
            | "Scintilla"
            | "TextArea"
            | "DirectUIHWND"
            | "SearchTextBox"
            | "Windows.UI.Core.CoreWindow"
            | "ApplicationFrameInputSinkWindow"
    ) {
        return true;
    }

    // Many modern desktop apps host editable controls under framework-specific
    // classes that are not stable enough for a strict allowlist. If another app
    // owns the foreground window and we have a concrete focus target, assume it
    // is safe enough to type into and reserve the purple warning for our own
    // overlay or true "no focus" states.
    true
}

#[cfg(target_os = "windows")]
fn window_class_name(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len as usize])
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

#[cfg(target_os = "windows")]
fn send_key_chord(chord: &str) -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
        VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_LWIN,
        VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
    };

    #[derive(Default)]
    struct Chord {
        ctrl: bool,
        alt: bool,
        shift: bool,
        logo: bool,
        key: Option<VIRTUAL_KEY>,
    }

    let mut parsed = Chord::default();
    for token in chord.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => parsed.ctrl = true,
            "alt" => parsed.alt = true,
            "shift" => parsed.shift = true,
            "win" | "meta" | "super" | "logo" => parsed.logo = true,
            "space" | "spacebar" => parsed.key = Some(VIRTUAL_KEY(VK_SPACE.0)),
            "enter" | "return" => parsed.key = Some(VIRTUAL_KEY(VK_RETURN.0)),
            "tab" => parsed.key = Some(VIRTUAL_KEY(VK_TAB.0)),
            "escape" | "esc" => parsed.key = Some(VIRTUAL_KEY(VK_ESCAPE.0)),
            "backspace" => parsed.key = Some(VIRTUAL_KEY(VK_BACK.0)),
            "delete" | "del" => parsed.key = Some(VIRTUAL_KEY(VK_DELETE.0)),
            "left" => parsed.key = Some(VIRTUAL_KEY(VK_LEFT.0)),
            "right" => parsed.key = Some(VIRTUAL_KEY(VK_RIGHT.0)),
            "up" => parsed.key = Some(VIRTUAL_KEY(VK_UP.0)),
            "down" => parsed.key = Some(VIRTUAL_KEY(VK_DOWN.0)),
            "home" => parsed.key = Some(VIRTUAL_KEY(VK_HOME.0)),
            "end" => parsed.key = Some(VIRTUAL_KEY(VK_END.0)),
            "pageup" => parsed.key = Some(VIRTUAL_KEY(VK_PRIOR.0)),
            "pagedown" => parsed.key = Some(VIRTUAL_KEY(VK_NEXT.0)),
            other if other.len() == 1 => {
                let byte = other.as_bytes()[0];
                if byte.is_ascii_alphanumeric() {
                    parsed.key = Some(VIRTUAL_KEY(byte as u16));
                }
            }
            _ => {}
        }
    }

    let key = parsed
        .key
        .ok_or_else(|| anyhow::anyhow!("unsupported key chord: {chord}"))?;
    let mut inputs = Vec::new();

    let mut push_key = |vk: VIRTUAL_KEY, keyup: bool| {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if keyup {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    };

    if parsed.ctrl {
        push_key(VIRTUAL_KEY(VK_CONTROL.0), false);
    }
    if parsed.alt {
        push_key(VIRTUAL_KEY(VK_MENU.0), false);
    }
    if parsed.shift {
        push_key(VIRTUAL_KEY(VK_SHIFT.0), false);
    }
    if parsed.logo {
        push_key(VIRTUAL_KEY(VK_LWIN.0), false);
    }

    push_key(key, false);
    push_key(key, true);

    if parsed.logo {
        push_key(VIRTUAL_KEY(VK_LWIN.0), true);
    }
    if parsed.shift {
        push_key(VIRTUAL_KEY(VK_SHIFT.0), true);
    }
    if parsed.alt {
        push_key(VIRTUAL_KEY(VK_MENU.0), true);
    }
    if parsed.ctrl {
        push_key(VIRTUAL_KEY(VK_CONTROL.0), true);
    }

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        anyhow::bail!(
            "SendInput typed only {sent} of {} chord events",
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

#[cfg(not(target_os = "windows"))]
fn has_text_entry_target() -> bool {
    true
}

#[cfg(not(target_os = "windows"))]
fn send_key_chord(_chord: &str) -> Result<()> {
    anyhow::bail!("key chords are only implemented on Windows")
}
