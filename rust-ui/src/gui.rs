use crate::icons::{AppIcon, handle as icon_handle};
use crate::model_download::{DownloadProgress, ensure_auto_model};
use iced::keyboard::{self, key};
use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, row, scrollable, svg, text, text_input,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Settings, Shadow, Subscription,
    Task, Theme, time, window,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use voice_typing_asr::{CurrentWiredModel, DesktopSherpaAsrService};
use voice_typing_core::{AsrResult, AsrService, TechAcronymMapper};
use voice_typing_platform_windows::{
    BackdropMaterial, BackdropPreference, TextInjector, resolve_backdrop,
};

// ── Layout ──────────────────────────────────────────────────────────────

const WIN_W: f32 = 142.0;
const WIN_H: f32 = 34.0;
const PANEL_W: f32 = 640.0;
const PANEL_H: f32 = 580.0;
const RADIUS: f32 = 12.0;

// ── Palette ─────────────────────────────────────────────────────────────

const SURFACE: Color = Color::from_rgba8(30, 30, 31, 0.99);
const SURFACE_EDGE: Color = Color::from_rgb8(36, 36, 38);
const HANDLE: Color = Color::from_rgb8(214, 214, 216);
const BLUE: Color = Color::from_rgb8(41, 121, 255);
const PURPLE: Color = Color::from_rgb8(173, 82, 255);
const RED: Color = Color::from_rgb8(255, 74, 74);
const GLYPH: Color = Color::from_rgb8(200, 200, 202);
const WHITE: Color = Color::WHITE;
const ORANGE: Color = Color::from_rgb8(255, 165, 0);
const PANEL_BG: Color = Color::from_rgba8(24, 24, 26, 0.985);
const IDLE_SUSPEND_AFTER: Duration = Duration::from_secs(10 * 60);
const RECOVERY_COOLDOWN: Duration = Duration::from_secs(6);
const AUDIO_STALL_TIMEOUT: Duration = Duration::from_secs(3);

// ── App state ───────────────────────────────────────────────────────────

pub fn run() -> iced::Result {
    iced::application(VoiceTypingApp::default, update, view)
        .title(app_title)
        .theme(app_theme)
        .window(window_settings())
        .subscription(subscription)
        .settings(Settings::default())
        .run()
}

fn app_title(_: &VoiceTypingApp) -> String {
    String::from("voice-typing")
}

fn app_theme(_: &VoiceTypingApp) -> Theme {
    Theme::TokyoNight
}

struct VoiceTypingApp {
    window_id: Option<window::Id>,
    mic: MicState,
    phase: f32,
    status: String,
    last_text: String,
    asr: Option<DesktopSherpaAsrService>,
    results: Option<tokio::sync::broadcast::Receiver<AsrResult>>,
    injector: TextInjector,
    last_injected: Option<(String, Instant)>,
    last_activity: Instant,
    mapper: TechAcronymMapper,
    last_recovery_attempt: Option<Instant>,
    audio_level: f32,
    target_warning_until: Option<Instant>,
    settings_open: bool,
    auto_off_enabled: bool,
    custom_entries: Vec<CustomMappingRow>,
    new_spoken: String,
    new_written: String,
    commands_enabled: bool,
    command_entries: Vec<VoiceCommandRow>,
    new_command_spoken: String,
    new_command_chord: String,
    capture_target: Option<CaptureTarget>,
    settings_tab: SettingsTab,
    model_mode: ModelMode,
    manual_model_path: String,
    backdrop_preference: BackdropPreference,
    applied_backdrop: Option<BackdropMaterial>,
    download_progress: Option<Arc<DownloadProgress>>,
    download_fraction: f32,
}

#[derive(Debug, Clone, Default)]
struct CustomMappingRow {
    spoken: String,
    written: String,
}

#[derive(Debug, Clone, Default)]
struct VoiceCommandRow {
    spoken: String,
    chord: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureTarget {
    Existing(usize),
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    VoiceMapping,
    VoiceExec,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MicState {
    Idle,
    Booting,
    Downloading,
    Active,
    Error,
}

#[derive(Debug, Clone)]
enum Msg {
    Opened(window::Id),
    Mic,
    Drag,
    Tick,
    SyncBackdrop,
    Bridge,
    BeginStart,
    AutoModelReady(Result<(), String>),
    ToggleSettings,
    SetAutoOff(bool),
    SelectBackdropPreference(BackdropPreference),
    EditSpoken(usize, String),
    EditWritten(usize, String),
    DeleteMapping(usize),
    NewSpokenChanged(String),
    NewWrittenChanged(String),
    AddMapping,
    SetCommandsEnabled(bool),
    EditCommandSpoken(usize, String),
    DeleteCommand(usize),
    NewCommandSpokenChanged(String),
    BeginCommandCapture(CaptureTarget),
    CapturedKey(keyboard::Event),
    AddCommand,
    SelectTab(SettingsTab),
    SelectModelMode(ModelMode),
    ManualModelPathChanged(String),
    CloseSettings,
    QuitAll,
}

impl Default for VoiceTypingApp {
    fn default() -> Self {
        let settings = load_app_settings(settings_path());
        let mappings = load_custom_mappings(user_dictionary_path());
        let commands = load_voice_commands(voice_commands_path());
        let mut mapper = TechAcronymMapper::new();
        let loaded_terms = mapper
            .load_user_corrections_file(user_dictionary_path())
            .unwrap_or(0);
        Self {
            window_id: None,
            mic: MicState::Idle,
            phase: 0.0,
            status: if loaded_terms > 0 {
                format!("idle ({loaded_terms} custom terms)")
            } else {
                String::from("idle")
            },
            last_text: String::new(),
            asr: None,
            results: None,
            injector: TextInjector::new(),
            last_injected: None,
            last_activity: Instant::now(),
            mapper,
            last_recovery_attempt: None,
            audio_level: 0.0,
            target_warning_until: None,
            settings_open: false,
            auto_off_enabled: settings.auto_off_enabled,
            custom_entries: mappings,
            new_spoken: String::new(),
            new_written: String::new(),
            commands_enabled: settings.commands_enabled,
            command_entries: commands,
            new_command_spoken: String::new(),
            new_command_chord: String::new(),
            capture_target: None,
            settings_tab: SettingsTab::General,
            model_mode: settings.model_mode,
            manual_model_path: settings.manual_model_path,
            backdrop_preference: settings.backdrop_preference,
            applied_backdrop: None,
            download_progress: None,
            download_fraction: 0.0,
        }
    }
}

// ── Subscriptions ───────────────────────────────────────────────────────

fn subscription(app: &VoiceTypingApp) -> Subscription<Msg> {
    let mut subs = vec![
        window::open_events().map(Msg::Opened),
        time::every(Duration::from_millis(250)).map(|_| Msg::Bridge),
    ];
    if matches!(app.backdrop_preference, BackdropPreference::FollowSystem) {
        subs.push(time::every(Duration::from_secs(2)).map(|_| Msg::SyncBackdrop));
    }
    if matches!(
        app.mic,
        MicState::Booting | MicState::Downloading | MicState::Active
    ) {
        subs.push(time::every(Duration::from_millis(30)).map(|_| Msg::Tick));
    }
    if app.capture_target.is_some() {
        subs.push(keyboard::listen().map(Msg::CapturedKey));
    }
    Subscription::batch(subs)
}

// ── Update ──────────────────────────────────────────────────────────────

fn update(app: &mut VoiceTypingApp, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::Opened(id) => {
            app.window_id = Some(id);
            return configure_window(app, id);
        }
        Msg::Mic => {
            if matches!(app.mic, MicState::Active) {
                stop_listening(app);
                app.mic = MicState::Idle;
                app.status = String::from("idle");
                app.audio_level = 0.0;
                app.download_progress = None;
                app.download_fraction = 0.0;
                app.target_warning_until = None;
                if let Some(b) = crate::bridge::get() {
                    b.set_state("idle");
                }
            } else {
                app.mic = MicState::Booting;
                app.status = String::from("booting");
                app.last_injected = None;
                app.last_activity = Instant::now();
                app.last_recovery_attempt = None;
                app.target_warning_until = None;
                app.capture_target = None;
                app.download_fraction = 0.0;
                if let Some(b) = crate::bridge::get() {
                    b.set_state("booting");
                }
                return Task::perform(async {}, |_| Msg::BeginStart);
            }
        }
        Msg::BeginStart => {
            if matches!(app.model_mode, ModelMode::Auto) && !CurrentWiredModel::auto_assets_ready()
            {
                let progress = Arc::new(DownloadProgress::default());
                app.download_progress = Some(Arc::clone(&progress));
                app.download_fraction = 0.0;
                app.mic = MicState::Downloading;
                app.status = format!(
                    "downloading model to {}",
                    CurrentWiredModel::auto_models_root().display()
                );
                if let Some(b) = crate::bridge::get() {
                    b.set_state("booting");
                }
                return Task::perform(ensure_auto_model(progress), |result| {
                    Msg::AutoModelReady(result.map_err(|err| err.to_string()))
                });
            }

            match start_asr(app) {
                Ok(()) => {
                    app.mic = MicState::Active;
                    app.status = String::from("listening");
                    if let Some(b) = crate::bridge::get() {
                        b.set_state("active");
                    }
                }
                Err(e) => {
                    app.mic = MicState::Error;
                    app.status = e;
                    app.audio_level = 0.0;
                    if let Some(b) = crate::bridge::get() {
                        b.set_state("idle");
                    }
                }
            }
        }
        Msg::AutoModelReady(result) => {
            app.download_progress = None;
            app.download_fraction = 0.0;

            match result {
                Ok(()) => match start_asr(app) {
                    Ok(()) => {
                        app.mic = MicState::Active;
                        app.status = String::from("listening");
                        if let Some(b) = crate::bridge::get() {
                            b.set_state("active");
                        }
                    }
                    Err(err) => {
                        app.mic = MicState::Error;
                        app.status = err;
                        app.audio_level = 0.0;
                        if let Some(b) = crate::bridge::get() {
                            b.set_state("idle");
                        }
                    }
                },
                Err(err) => {
                    app.mic = MicState::Error;
                    app.status = format!("model download failed: {err}");
                    app.audio_level = 0.0;
                    if let Some(b) = crate::bridge::get() {
                        b.set_state("idle");
                    }
                }
            }
        }
        Msg::Drag => {
            if let Some(id) = app.window_id {
                return window::drag(id);
            }
        }
        Msg::Tick => {
            app.phase = (app.phase + 0.055) % 1.0;
            if let Some(progress) = app.download_progress.as_ref() {
                app.download_fraction = progress.fraction();
                if progress.is_running() {
                    app.status = format!(
                        "downloading model {:.0}%",
                        (app.download_fraction * 100.0).clamp(0.0, 100.0)
                    );
                }
            }
            sync_audio_level(app);
            if matches!(app.mic, MicState::Active)
                && app.audio_level > 0.035
                && !app.injector.has_text_entry_target()
            {
                app.target_warning_until = Some(Instant::now() + Duration::from_millis(220));
            }
            if app
                .target_warning_until
                .is_some_and(|until| Instant::now() >= until)
            {
                app.target_warning_until = None;
            }
            if matches!(app.mic, MicState::Active)
                && app.auto_off_enabled
                && app.last_activity.elapsed() >= IDLE_SUSPEND_AFTER
            {
                stop_listening(app);
                app.mic = MicState::Idle;
                app.status = String::from("pipeline suspended after inactivity");
                app.audio_level = 0.0;
                app.target_warning_until = None;
                if let Some(b) = crate::bridge::get() {
                    b.set_state("idle");
                }
                return Task::none();
            }
            drain_results(app);
            maybe_recover_session(app);
            if let Some(task) = sync_backdrop(app) {
                return task;
            }
        }
        Msg::SyncBackdrop => {
            if let Some(task) = sync_backdrop(app) {
                return task;
            }
        }
        Msg::Bridge => {
            if let Some(b) = crate::bridge::get() {
                if let Some(cmd) = b.try_recv_command() {
                    match cmd {
                        crate::bridge::ExtensionCommand::ToggleListening => {
                            return update(app, Msg::Mic);
                        }
                    }
                }
            }
        }
        Msg::ToggleSettings => return toggle_settings(app),
        Msg::CloseSettings => return set_settings_open(app, false),
        Msg::SetAutoOff(value) => {
            app.auto_off_enabled = value;
            persist_app_settings(app);
        }
        Msg::SelectBackdropPreference(value) => {
            app.backdrop_preference = value;
            persist_app_settings(app);
            if let Some(task) = sync_backdrop(app) {
                return task;
            }
        }
        Msg::EditSpoken(index, value) => {
            if let Some(row) = app.custom_entries.get_mut(index) {
                row.spoken = value;
                persist_custom_mappings(app);
            }
        }
        Msg::EditWritten(index, value) => {
            if let Some(row) = app.custom_entries.get_mut(index) {
                row.written = value;
                persist_custom_mappings(app);
            }
        }
        Msg::DeleteMapping(index) => {
            if index < app.custom_entries.len() {
                app.custom_entries.remove(index);
                persist_custom_mappings(app);
            }
        }
        Msg::NewSpokenChanged(value) => app.new_spoken = value,
        Msg::NewWrittenChanged(value) => app.new_written = value,
        Msg::AddMapping => {
            let spoken = app.new_spoken.trim();
            let written = app.new_written.trim();
            if !spoken.is_empty() && !written.is_empty() {
                app.custom_entries.push(CustomMappingRow {
                    spoken: spoken.to_owned(),
                    written: written.to_owned(),
                });
                app.new_spoken.clear();
                app.new_written.clear();
                persist_custom_mappings(app);
            }
        }
        Msg::SetCommandsEnabled(value) => {
            app.commands_enabled = value;
            if !value && matches!(app.settings_tab, SettingsTab::VoiceExec) {
                app.settings_tab = SettingsTab::General;
            }
            persist_app_settings(app);
        }
        Msg::EditCommandSpoken(index, value) => {
            if let Some(row) = app.command_entries.get_mut(index) {
                row.spoken = value;
                persist_voice_commands(app);
            }
        }
        Msg::DeleteCommand(index) => {
            if index < app.command_entries.len() {
                app.command_entries.remove(index);
                persist_voice_commands(app);
            }
        }
        Msg::NewCommandSpokenChanged(value) => app.new_command_spoken = value,
        Msg::BeginCommandCapture(target) => {
            app.capture_target = Some(target);
            app.status = String::from("press shortcut");
        }
        Msg::CapturedKey(event) => {
            if let Some(target) = app.capture_target
                && let Some(chord) = capture_chord_from_event(&event)
            {
                match target {
                    CaptureTarget::Existing(index) => {
                        if let Some(row) = app.command_entries.get_mut(index) {
                            row.chord = chord;
                            persist_voice_commands(app);
                        }
                    }
                    CaptureTarget::New => {
                        app.new_command_chord = chord;
                    }
                }
                app.capture_target = None;
            }
        }
        Msg::AddCommand => {
            let spoken = app.new_command_spoken.trim();
            let chord = app.new_command_chord.trim();
            if !spoken.is_empty() && !chord.is_empty() {
                app.command_entries.push(VoiceCommandRow {
                    spoken: spoken.to_owned(),
                    chord: chord.to_owned(),
                });
                app.new_command_spoken.clear();
                app.new_command_chord.clear();
                persist_voice_commands(app);
            }
        }
        Msg::SelectTab(tab) => app.settings_tab = tab,
        Msg::SelectModelMode(mode) => {
            app.model_mode = mode;
            app.asr = None;
            persist_app_settings(app);
        }
        Msg::ManualModelPathChanged(value) => {
            app.manual_model_path = value;
            app.asr = None;
            persist_app_settings(app);
        }
        Msg::QuitAll => {
            stop_listening(app);
            return iced::exit();
        }
    }
    Task::none()
}

// ── View ────────────────────────────────────────────────────────────────

fn view(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let chrome = row![
        container(left_handle())
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(20.0))
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 8.0,
            })
            .align_left(Length::Fill)
            .center_y(Length::Fill),
        container(mic_btn(app))
            .width(Length::Fill)
            .height(Length::Fixed(20.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        container(trailing_menu())
            .width(Length::Fixed(44.0))
            .height(Length::Fixed(WIN_H))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .padding(Padding {
        top: 6.0,
        right: 8.0,
        bottom: 6.0,
        left: 0.0,
    });

    let shell = mouse_area(
        container(chrome)
            .width(Length::Fill)
            .height(Length::Fixed(WIN_H))
            .style(move |_| shell_style(app)),
    )
    .on_press(Msg::Drag);

    if app.settings_open {
        let panel = settings_panel(app);
        container(column![shell, panel].spacing(10))
            .width(Length::Fill)
            .height(Length::Shrink)
            .padding(Padding::from(6))
            .into()
    } else {
        container(shell)
            .width(Length::Fill)
            .height(Length::Shrink)
            .padding(Padding::from(0))
            .into()
    }
}

// ── Mic button (styled round button with SVG icon) ─────────────────────

fn mic_btn(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let active_glow = active_audio_glow(app);
    let mic_ring = ring_color(app);
    let mic_color = match app.mic {
        MicState::Active => mix_color(mic_ring, WHITE, 0.18 + active_glow * 0.55),
        MicState::Booting => ORANGE,
        MicState::Downloading => mix_color(ORANGE, WHITE, app.download_fraction * 0.35),
        MicState::Error => RED,
        MicState::Idle => WHITE,
    };

    let content: Element<'_, Msg> = if matches!(app.mic, MicState::Downloading) {
        text(pizza_glyph(app.download_fraction))
            .size(22)
            .color(mic_color)
            .into()
    } else {
        svg(icon_handle(AppIcon::Mic))
            .width(Length::Fixed(20.7 + active_glow * 2.1))
            .height(Length::Fixed(20.7 + active_glow * 2.1))
            .style(move |_, _| svg::Style {
                color: Some(mic_color),
            })
            .into()
    };

    button(content)
        .on_press(Msg::Mic)
        .padding(0)
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .style(move |_, _| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: WHITE,
            border: Border {
                radius: 999.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow::default(),
            ..Default::default()
        })
        .into()
}

// ── Styles ──────────────────────────────────────────────────────────────

fn shell_style(app: &VoiceTypingApp) -> iced::widget::container::Style {
    let glow = active_audio_glow(app);
    let material = current_backdrop_material(app);
    let idle_border = match material {
        BackdropMaterial::Acrylic => Color::from_rgba8(255, 255, 255, 0.36),
        BackdropMaterial::Mica => Color::from_rgba8(96, 112, 140, 0.42),
    };
    let border_color = match app.mic {
        MicState::Active => mix_color(ring_color(app), WHITE, 0.18 + glow * 0.72),
        MicState::Booting => mix_color(ORANGE, WHITE, 0.25),
        MicState::Downloading => mix_color(ORANGE, WHITE, app.download_fraction * 0.5),
        MicState::Error => mix_color(RED, WHITE, 0.18),
        MicState::Idle => idle_border,
    };
    let border_width = match app.mic {
        MicState::Active => 1.0 + glow * 1.4,
        MicState::Booting => 1.2,
        MicState::Downloading => 1.3,
        MicState::Error => 1.3,
        MicState::Idle => 1.0,
    };
    let shadow_color = match app.mic {
        MicState::Active => {
            let tint = mix_color(ring_color(app), WHITE, glow * 0.55);
            Color::from_rgba(tint.r, tint.g, tint.b, 0.14 + glow * 0.18)
        }
        MicState::Booting => Color::from_rgba(ORANGE.r, ORANGE.g, ORANGE.b, 0.18),
        MicState::Downloading => {
            let tint = mix_color(ORANGE, WHITE, app.download_fraction * 0.55);
            Color::from_rgba(tint.r, tint.g, tint.b, 0.2 + app.download_fraction * 0.12)
        }
        MicState::Error => Color::from_rgba(RED.r, RED.g, RED.b, 0.22),
        MicState::Idle => match material {
            BackdropMaterial::Acrylic => Color::from_rgba8(235, 245, 255, 0.14),
            BackdropMaterial::Mica => Color::from_rgba8(18, 28, 42, 0.24),
        },
    };

    iced::widget::container::Style {
        background: Some(Background::Color(widget_surface())),
        border: Border {
            radius: RADIUS.into(),
            width: border_width,
            color: border_color,
        },
        shadow: Shadow {
            color: shadow_color,
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0 + glow * 10.0,
        },
        ..Default::default()
    }
}

fn left_handle<'a>() -> Element<'a, Msg> {
    container(Space::new())
        .width(Length::Fixed(4.0))
        .height(Length::Fixed(14.0))
        .style(|_| iced::widget::container::Style {
            background: Some(Background::Color(HANDLE)),
            border: Border {
                radius: 99.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .into()
}

fn trailing_menu<'a>() -> Element<'a, Msg> {
    container(
        button(
            text("…")
                .size(20)
                .line_height(iced::widget::text::LineHeight::Relative(1.0))
                .color(GLYPH),
        )
        .on_press(Msg::ToggleSettings)
        .style(|_, _| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: WHITE,
            border: Border {
                radius: 999.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow::default(),
            ..Default::default()
        })
        .padding(0),
    )
    .width(Length::Fixed(40.0))
    .height(Length::Fixed(WIN_H))
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    })
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

// ── Window setup ────────────────────────────────────────────────────────

fn window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(WIN_W, WIN_H),
        min_size: Some(iced::Size::new(WIN_W, WIN_H)),
        level: window::Level::AlwaysOnTop,
        decorations: false,
        resizable: false,
        transparent: true,
        blur: cfg!(target_os = "macos"),
        platform_specific: platform_window(),
        ..window::Settings::default()
    }
}

#[cfg(target_os = "windows")]
fn platform_window() -> window::settings::PlatformSpecific {
    use iced::window::settings::{PlatformSpecific, platform::CornerPreference};
    PlatformSpecific {
        undecorated_shadow: true,
        corner_preference: CornerPreference::Round,
        ..PlatformSpecific::default()
    }
}

#[cfg(target_os = "macos")]
fn platform_window() -> window::settings::PlatformSpecific {
    use iced::window::settings::PlatformSpecific;
    PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_window() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific::default()
}

fn configure_window(app: &mut VoiceTypingApp, id: window::Id) -> Task<Msg> {
    #[cfg(target_os = "windows")]
    {
        let material = resolve_backdrop(app.backdrop_preference);
        app.applied_backdrop = Some(material);
        return window::run(id, move |h| {
            let _ = voice_typing_platform_windows::apply_backdrop(h, material);
        })
        .discard();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = id;
        Task::none()
    }
}

fn sync_backdrop(app: &mut VoiceTypingApp) -> Option<Task<Msg>> {
    #[cfg(target_os = "windows")]
    {
        let id = app.window_id?;
        let material = resolve_backdrop(app.backdrop_preference);
        if app.applied_backdrop == Some(material) {
            return None;
        }
        app.applied_backdrop = Some(material);
        Some(
            window::run(id, move |h| {
                let _ = voice_typing_platform_windows::apply_backdrop(h, material);
            })
            .discard(),
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        None
    }
}

// ── ASR helpers ─────────────────────────────────────────────────────────

fn start_asr(app: &mut VoiceTypingApp) -> Result<(), String> {
    if app.asr.is_none() {
        let mut asr = DesktopSherpaAsrService::new();
        let model_path = selected_model_path(app);
        asr.initialize_blocking(&model_path)
            .map_err(|e| format!("{e}"))?;
        app.asr = Some(asr);
    }
    let asr = app.asr.as_mut().ok_or("unavailable")?;
    app.results = Some(asr.subscribe_results());
    asr.start_real_time_session().map_err(|e| format!("{e}"))
}

fn stop_listening(app: &mut VoiceTypingApp) {
    if let Some(asr) = app.asr.as_mut() {
        let _ = asr.stop_real_time_session();
    }
    app.results = None;
    app.last_injected = None;
    app.audio_level = 0.0;
}

fn maybe_recover_session(app: &mut VoiceTypingApp) {
    let Some(asr) = app.asr.as_mut() else {
        return;
    };

    let health = asr.session_health();
    if !health.worker_running {
        return;
    }

    let now = Instant::now();
    if app
        .last_recovery_attempt
        .is_some_and(|attempt| now.duration_since(attempt) < RECOVERY_COOLDOWN)
    {
        return;
    }

    let audio_stalled = health
        .last_audio_age
        .is_some_and(|age| age >= AUDIO_STALL_TIMEOUT);
    let stream_failed = health.last_error.is_some();

    if !(audio_stalled || stream_failed) {
        return;
    }

    let reason = health
        .last_error
        .unwrap_or_else(|| "microphone stalled; restarting".to_owned());
    app.status = format!("recovering: {reason}");
    app.last_recovery_attempt = Some(now);

    let _ = asr.stop_real_time_session();
    app.results = Some(asr.subscribe_results());
    match asr.start_real_time_session() {
        Ok(()) => {
            app.status = String::from("listening");
            app.last_activity = Instant::now();
            if let Some(b) = crate::bridge::get() {
                b.set_state("active");
            }
        }
        Err(err) => {
            app.mic = MicState::Error;
            app.status = format!("mic recovery failed: {err}");
            app.audio_level = 0.0;
            app.target_warning_until = None;
            if let Some(b) = crate::bridge::get() {
                b.set_state("idle");
            }
        }
    }
}

fn sync_audio_level(app: &mut VoiceTypingApp) {
    let target = app
        .asr
        .as_ref()
        .map(|asr| asr.session_health().audio_level)
        .unwrap_or(0.0);

    let attack = 0.55;
    let release = 0.14;
    let smoothing = if target >= app.audio_level {
        attack
    } else {
        release
    };

    app.audio_level += (target - app.audio_level) * smoothing;
    if app.audio_level < 0.002 {
        app.audio_level = 0.0;
    }
}

fn drain_results(app: &mut VoiceTypingApp) {
    let mut received = Vec::new();
    if let Some(rx) = app.results.as_mut()
        && matches!(app.mic, MicState::Active)
    {
        while let Ok(r) = rx.try_recv() {
            received.push(r);
        }
    }
    for r in received {
        if !r.is_final {
            continue;
        }
        let t = r.text.trim();
        if t.is_empty() || is_dup(app, t) {
            continue;
        }
        let mapped = app.mapper.map(t);
        app.last_text = mapped.clone();
        app.last_activity = Instant::now();
        if let Some(b) = crate::bridge::get() {
            b.send_transcript(&mapped, r.is_final);
        }
        if app.commands_enabled
            && let Some(command) = find_voice_command(app, &mapped)
        {
            if let Err(err) = app
                .injector
                .send_key_chord_to_focused_window(&command.chord)
            {
                app.status = format!("command err: {err}");
            } else {
                app.status = format!("sent {}", command.chord);
                app.last_injected = Some((mapped.clone(), Instant::now()));
            }
            continue;
        }
        if !app.injector.has_text_entry_target() {
            app.status = String::from("no text field selected");
            app.target_warning_until = Some(Instant::now() + Duration::from_millis(700));
            continue;
        }
        app.status = format!("sent {}", clock());
        app.last_injected = Some((mapped.clone(), Instant::now()));
        if let Err(e) = app.injector.send_to_focused_window(&mapped) {
            app.status = format!("err: {e}");
        }
    }
}

fn is_dup(app: &VoiceTypingApp, t: &str) -> bool {
    matches!(&app.last_injected, Some((prev, at)) if prev == t && at.elapsed() < Duration::from_secs(2))
}

fn clock() -> String {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{:02}:{:02}", (s / 60) % 60, s % 60)
}

fn app_data_dir() -> PathBuf {
    CurrentWiredModel::voice_typing_home()
}

fn user_dictionary_path() -> PathBuf {
    app_data_dir().join("user_dictionary.txt")
}

fn toggle_settings(app: &mut VoiceTypingApp) -> Task<Msg> {
    set_settings_open(app, !app.settings_open)
}

fn set_settings_open(app: &mut VoiceTypingApp, open: bool) -> Task<Msg> {
    app.settings_open = open;
    let Some(id) = app.window_id else {
        return Task::none();
    };
    let size = if open {
        iced::Size::new(PANEL_W, PANEL_H)
    } else {
        iced::Size::new(WIN_W, WIN_H)
    };
    window::resize(id, size)
}

fn settings_panel(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let mut tabs = row![
        tab_button("General", SettingsTab::General, app.settings_tab),
        tab_button("Voice Mapping", SettingsTab::VoiceMapping, app.settings_tab),
    ]
    .spacing(0)
    .align_y(Alignment::End);

    if app.commands_enabled {
        tabs = tabs.push(tab_button(
            "Voice Exec",
            SettingsTab::VoiceExec,
            app.settings_tab,
        ));
    }

    tabs = tabs.push(tab_button("Model", SettingsTab::Model, app.settings_tab));

    let body = match app.settings_tab {
        SettingsTab::General => general_tab(app),
        SettingsTab::VoiceMapping => voice_mapping_tab(app),
        SettingsTab::VoiceExec => voice_exec_tab(app),
        SettingsTab::Model => model_tab(app),
    };

    let rows = column![
        text("Settings").size(20).color(WHITE),
        tabs,
        container(scrollable(body).height(Length::Fill))
            .width(Length::Fill)
            .style(|_| panel_inner_style(app)),
        row![
            button(text("Quit All").size(14))
                .on_press(Msg::QuitAll)
                .style(|_, _| pill_button_style(true)),
            Space::new().width(Length::Fill),
            button(text("Done").size(14))
                .on_press(Msg::CloseSettings)
                .style(|_, _| pill_button_style(false)),
        ]
        .align_y(Alignment::Center)
        .spacing(10),
    ]
    .spacing(12);

    container(rows.padding(16))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| panel_style(app))
        .into()
}

fn general_tab(app: &VoiceTypingApp) -> Element<'_, Msg> {
    column![
        checkbox(app.auto_off_enabled)
            .label("Turn off after 10m no voice detected")
            .on_toggle(Msg::SetAutoOff)
            .spacing(10)
            .size(16),
        checkbox(app.commands_enabled)
            .label("Voice command exec")
            .on_toggle(Msg::SetCommandsEnabled)
            .spacing(10)
            .size(16),
        text("Material:").size(14).color(GLYPH),
        setting_radio(
            "Light",
            "Acrylic",
            BackdropPreference::Light,
            app.backdrop_preference
        ),
        setting_radio(
            "Dark",
            "Mica",
            BackdropPreference::Dark,
            app.backdrop_preference
        ),
        setting_radio(
            "Follow System",
            "Switches with Windows theme",
            BackdropPreference::FollowSystem,
            app.backdrop_preference,
        ),
    ]
    .spacing(14)
    .padding(12)
    .into()
}

fn voice_mapping_tab(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let mut mapping_table = column![mapping_header(), new_mapping_row(app)].spacing(6);
    for (index, row_data) in app.custom_entries.iter().enumerate() {
        mapping_table = mapping_table.push(mapping_row(index, row_data));
    }

    let content = column![
        text("Utterance to text mappings").size(14).color(GLYPH),
        mapping_table
    ]
    .spacing(10);

    container(content.spacing(12).padding(12))
        .width(Length::Fill)
        .into()
}

fn voice_exec_tab(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let mut command_table = column![command_header(), new_command_row(app)].spacing(6);
    for (index, row_data) in app.command_entries.iter().enumerate() {
        command_table = command_table.push(command_row(index, row_data, app.capture_target));
    }

    container(
        column![
            text("Utterance to keypress").size(14).color(GLYPH),
            command_table
        ]
        .spacing(10)
        .padding(12),
    )
    .width(Length::Fill)
    .into()
}

fn model_tab(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let auto_label = format!("Auto  ({})", current_model_label());

    container(
        column![
            model_radio(auto_label, ModelMode::Auto, app.model_mode),
            model_radio("Manual".to_owned(), ModelMode::Manual, app.model_mode),
            text_input("path to model directory", &app.manual_model_path)
                .on_input(Msg::ManualModelPathChanged)
                .padding(10)
                .size(13)
                .style(|_, _| text_field_style())
                .width(Length::Fill),
            text("Manual mode expects a folder containing encoder_model.ort, decoder_model_merged.ort, and tokens.txt.")
                .size(12)
                .color(GLYPH),
        ]
        .spacing(12),
    )
    .padding(12)
    .width(Length::Fill)
    .into()
}

fn mapping_header<'a>() -> Element<'a, Msg> {
    row![
        container(text("Speak").size(12).color(GLYPH)).width(Length::FillPortion(4)),
        container(text("Type").size(12).color(GLYPH)).width(Length::FillPortion(4)),
        container(Space::new()).width(Length::Fixed(72.0)),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn mapping_row<'a>(index: usize, row_data: &CustomMappingRow) -> Element<'a, Msg> {
    row![
        text_input("spoken phrase", &row_data.spoken)
            .on_input(move |value| Msg::EditSpoken(index, value))
            .padding(8)
            .size(13)
            .style(|_, _| text_field_style())
            .width(Length::FillPortion(4)),
        text_input("replacement", &row_data.written)
            .on_input(move |value| Msg::EditWritten(index, value))
            .padding(8)
            .size(13)
            .style(|_, _| text_field_style())
            .width(Length::FillPortion(4)),
        button(text("Delete").size(12))
            .on_press(Msg::DeleteMapping(index))
            .style(|_, _| pill_button_style(true))
            .width(Length::Fixed(72.0)),
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .into()
}

fn new_mapping_row<'a>(app: &VoiceTypingApp) -> Element<'a, Msg> {
    row![
        text_input("new spoken phrase", &app.new_spoken)
            .on_input(Msg::NewSpokenChanged)
            .padding(8)
            .size(13)
            .style(|_, _| text_field_style())
            .width(Length::FillPortion(4)),
        text_input("new replacement", &app.new_written)
            .on_input(Msg::NewWrittenChanged)
            .padding(8)
            .size(13)
            .style(|_, _| text_field_style())
            .width(Length::FillPortion(4)),
        button(text("Add").size(12))
            .on_press(Msg::AddMapping)
            .style(|_, _| pill_button_style(false))
            .width(Length::Fixed(72.0)),
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .into()
}

fn command_header<'a>() -> Element<'a, Msg> {
    row![
        container(text("Speak").size(12).color(GLYPH)).width(Length::FillPortion(4)),
        container(text("Keys").size(12).color(GLYPH)).width(Length::FillPortion(4)),
        container(Space::new()).width(Length::Fixed(72.0)),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn command_row<'a>(
    index: usize,
    row_data: &VoiceCommandRow,
    capture_target: Option<CaptureTarget>,
) -> Element<'a, Msg> {
    let is_capturing = capture_target == Some(CaptureTarget::Existing(index));
    let label = if is_capturing {
        "Press keys…".to_owned()
    } else if row_data.chord.trim().is_empty() {
        "Record shortcut".to_owned()
    } else {
        row_data.chord.clone()
    };

    row![
        text_input("spoken phrase", &row_data.spoken)
            .on_input(move |value| Msg::EditCommandSpoken(index, value))
            .padding(8)
            .size(13)
            .style(|_, _| text_field_style())
            .width(Length::FillPortion(4)),
        button(text(label).size(13))
            .on_press(Msg::BeginCommandCapture(CaptureTarget::Existing(index)))
            .style(|_, _| text_field_button_style())
            .padding(Padding::from(8))
            .width(Length::FillPortion(4)),
        button(text("Delete").size(12))
            .on_press(Msg::DeleteCommand(index))
            .style(|_, _| pill_button_style(true))
            .width(Length::Fixed(72.0)),
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .into()
}

fn new_command_row<'a>(app: &VoiceTypingApp) -> Element<'a, Msg> {
    let is_capturing = app.capture_target == Some(CaptureTarget::New);
    let label = if is_capturing {
        "Press keys…".to_owned()
    } else if app.new_command_chord.trim().is_empty() {
        "Record shortcut".to_owned()
    } else {
        app.new_command_chord.clone()
    };

    row![
        text_input("new spoken phrase", &app.new_command_spoken)
            .on_input(Msg::NewCommandSpokenChanged)
            .padding(8)
            .size(13)
            .style(|_, _| text_field_style())
            .width(Length::FillPortion(4)),
        button(text(label).size(13))
            .on_press(Msg::BeginCommandCapture(CaptureTarget::New))
            .style(|_, _| text_field_button_style())
            .padding(Padding::from(8))
            .width(Length::FillPortion(4)),
        button(text("Add").size(12))
            .on_press(Msg::AddCommand)
            .style(|_, _| pill_button_style(false))
            .width(Length::Fixed(72.0)),
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .into()
}

fn panel_style(app: &VoiceTypingApp) -> iced::widget::container::Style {
    let material = current_backdrop_material(app);
    let border_color = match material {
        BackdropMaterial::Acrylic => Color::from_rgba8(255, 255, 255, 0.28),
        BackdropMaterial::Mica => Color::from_rgba8(88, 102, 130, 0.42),
    };
    let shadow_color = match material {
        BackdropMaterial::Acrylic => Color::from_rgba8(220, 235, 255, 0.1),
        BackdropMaterial::Mica => Color::from_rgba8(0, 0, 0, 0.22),
    };
    iced::widget::container::Style {
        background: Some(Background::Color(panel_surface())),
        border: Border {
            radius: 18.0.into(),
            width: 1.0,
            color: border_color,
        },
        shadow: Shadow {
            color: shadow_color,
            offset: iced::Vector::new(0.0, 10.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    }
}

fn panel_inner_style(app: &VoiceTypingApp) -> iced::widget::container::Style {
    let border_color = match current_backdrop_material(app) {
        BackdropMaterial::Acrylic => Color::from_rgba8(255, 255, 255, 0.2),
        BackdropMaterial::Mica => Color::from_rgba8(74, 82, 102, 0.36),
    };
    iced::widget::container::Style {
        background: Some(Background::Color(panel_inner_surface())),
        border: Border {
            radius: 12.0.into(),
            width: 1.0,
            color: border_color,
        },
        ..Default::default()
    }
}

fn text_field_style() -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: Background::Color(Color::from_rgb8(32, 32, 36)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: Color::from_rgb8(62, 62, 68),
        },
        icon: WHITE,
        placeholder: Color::from_rgb8(128, 128, 133),
        value: WHITE,
        selection: Color::from_rgba8(41, 121, 255, 0.35),
    }
}

fn text_field_button_style() -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: Some(Background::Color(Color::from_rgb8(32, 32, 36))),
        text_color: WHITE,
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: Color::from_rgb8(62, 62, 68),
        },
        shadow: Shadow::default(),
        ..Default::default()
    }
}

fn tab_button<'a>(label: &'a str, tab: SettingsTab, current: SettingsTab) -> Element<'a, Msg> {
    let selected = tab == current;
    button(text(label).size(13))
        .on_press(Msg::SelectTab(tab))
        .style(move |_, status| tab_button_style(selected, status))
        .padding(Padding {
            top: 10.0,
            right: 14.0,
            bottom: 9.0,
            left: 14.0,
        })
        .into()
}

fn model_radio(label: String, mode: ModelMode, current: ModelMode) -> Element<'static, Msg> {
    let selected = mode == current;
    button(
        row![
            text(if selected { "●" } else { "○" }).size(16).color(WHITE),
            text(label).size(13).color(WHITE),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(Msg::SelectModelMode(mode))
    .style(|_, _| text_field_button_style())
    .padding(Padding {
        top: 10.0,
        right: 12.0,
        bottom: 10.0,
        left: 12.0,
    })
    .width(Length::Fill)
    .into()
}

fn setting_radio<'a>(
    label: &'a str,
    subtitle: &'a str,
    value: BackdropPreference,
    current: BackdropPreference,
) -> Element<'a, Msg> {
    let selected = value == current;
    button(
        row![
            text(if selected { "●" } else { "○" }).size(16).color(WHITE),
            column![
                text(label).size(13).color(WHITE),
                text(subtitle).size(11).color(GLYPH),
            ]
            .spacing(2),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(Msg::SelectBackdropPreference(value))
    .style(|_, _| text_field_button_style())
    .padding(Padding {
        top: 10.0,
        right: 12.0,
        bottom: 10.0,
        left: 12.0,
    })
    .width(Length::Fill)
    .into()
}

fn pill_button_style(danger: bool) -> iced::widget::button::Style {
    let bg = if danger {
        Color::from_rgb8(80, 34, 38)
    } else {
        Color::from_rgb8(44, 44, 50)
    };
    let border = if danger {
        Color::from_rgb8(138, 54, 64)
    } else {
        Color::from_rgb8(72, 72, 78)
    };

    iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color: WHITE,
        border: Border {
            radius: 999.0.into(),
            width: 1.0,
            color: border,
        },
        shadow: Shadow::default(),
        ..Default::default()
    }
}

fn tab_button_style(
    selected: bool,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let bg = if selected {
        Color::from_rgb8(42, 42, 48)
    } else {
        Color::from_rgba8(0, 0, 0, 0.0)
    };
    let border = if selected {
        Color::from_rgb8(74, 74, 84)
    } else {
        Color::from_rgb8(48, 48, 54)
    };
    let text_color = if selected {
        WHITE
    } else {
        mix_color(GLYPH, WHITE, 0.12)
    };

    let mut style = iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            radius: iced::border::top(12.0).left(12.0).right(12.0),
            width: 1.0,
            color: border,
        },
        shadow: Shadow::default(),
        ..Default::default()
    };

    if matches!(status, iced::widget::button::Status::Hovered) && !selected {
        style.background = Some(Background::Color(Color::from_rgba8(255, 255, 255, 0.03)));
        style.text_color = WHITE;
    }

    style
}

fn load_custom_mappings(path: PathBuf) -> Vec<CustomMappingRow> {
    let Ok(content) = read_text_with_legacy(path, "user_dictionary.txt") else {
        return Vec::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || !line.contains("=>") {
                return None;
            }
            let mut parts = line.splitn(2, "=>");
            let spoken = parts.next()?.trim();
            let written = parts.next()?.trim();
            if spoken.is_empty() || written.is_empty() {
                return None;
            }
            Some(CustomMappingRow {
                spoken: spoken.to_owned(),
                written: written.to_owned(),
            })
        })
        .collect()
}

fn persist_custom_mappings(app: &mut VoiceTypingApp) {
    let body = app
        .custom_entries
        .iter()
        .filter(|entry| !entry.spoken.trim().is_empty() && !entry.written.trim().is_empty())
        .map(|entry| format!("{} => {}", entry.spoken.trim(), entry.written.trim()))
        .collect::<Vec<_>>()
        .join("\n");

    let content = if body.is_empty() {
        String::from("# Custom utterance mappings\n")
    } else {
        format!("# Custom utterance mappings\n{body}\n")
    };

    if let Err(err) = fs::create_dir_all(app_data_dir()) {
        app.status = format!("save failed: {err}");
        return;
    }

    match fs::write(user_dictionary_path(), content) {
        Ok(()) => {
            let mut mapper = TechAcronymMapper::new();
            let loaded = mapper
                .load_user_corrections_file(user_dictionary_path())
                .unwrap_or(0);
            app.mapper = mapper;
            app.status = format!("saved {loaded} mappings");
        }
        Err(err) => {
            app.status = format!("save failed: {err}");
        }
    }
}

fn voice_commands_path() -> PathBuf {
    app_data_dir().join("voice_commands.txt")
}

fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

struct AppSettings {
    auto_off_enabled: bool,
    commands_enabled: bool,
    model_mode: ModelMode,
    manual_model_path: String,
    backdrop_preference: BackdropPreference,
}

fn load_app_settings(path: PathBuf) -> AppSettings {
    let Ok(content) = read_text_with_legacy(path, "settings.json") else {
        return default_app_settings();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return default_app_settings();
    };

    AppSettings {
        auto_off_enabled: value
            .get("auto_off_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        commands_enabled: value
            .get("commands_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        model_mode: match value.get("model_mode").and_then(|v| v.as_str()) {
            Some("manual") => ModelMode::Manual,
            _ => ModelMode::Auto,
        },
        manual_model_path: value
            .get("manual_model_path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned(),
        backdrop_preference: match value.get("backdrop_preference").and_then(|v| v.as_str()) {
            Some("light") => BackdropPreference::Light,
            Some("dark") => BackdropPreference::Dark,
            _ => BackdropPreference::FollowSystem,
        },
    }
}

fn default_app_settings() -> AppSettings {
    AppSettings {
        auto_off_enabled: true,
        commands_enabled: true,
        model_mode: ModelMode::Auto,
        manual_model_path: String::new(),
        backdrop_preference: BackdropPreference::FollowSystem,
    }
}

fn persist_app_settings(app: &mut VoiceTypingApp) {
    let value = serde_json::json!({
        "auto_off_enabled": app.auto_off_enabled,
        "commands_enabled": app.commands_enabled,
        "model_mode": if matches!(app.model_mode, ModelMode::Manual) { "manual" } else { "auto" },
        "manual_model_path": app.manual_model_path,
        "backdrop_preference": match app.backdrop_preference {
            BackdropPreference::Light => "light",
            BackdropPreference::Dark => "dark",
            BackdropPreference::FollowSystem => "follow_system",
        },
    });

    if let Err(err) = fs::create_dir_all(app_data_dir()) {
        app.status = format!("settings save failed: {err}");
        return;
    }

    if let Err(err) = fs::write(settings_path(), format!("{value:#}")) {
        app.status = format!("settings save failed: {err}");
    }
}

fn selected_model_path(app: &VoiceTypingApp) -> String {
    match app.model_mode {
        ModelMode::Auto => CurrentWiredModel::auto_model_dir().display().to_string(),
        ModelMode::Manual => {
            let trimmed = app.manual_model_path.trim();
            if trimmed.is_empty() {
                CurrentWiredModel::MODEL_DIR.to_owned()
            } else {
                trimmed.to_owned()
            }
        }
    }
}

fn current_model_label() -> String {
    CurrentWiredModel::auto_model_dir()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CurrentWiredModel::MODEL_NAME)
        .to_owned()
}

fn load_voice_commands(path: PathBuf) -> Vec<VoiceCommandRow> {
    let Ok(content) = read_text_with_legacy(path, "voice_commands.txt") else {
        return Vec::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || !line.contains("=>") {
                return None;
            }
            let mut parts = line.splitn(2, "=>");
            let spoken = parts.next()?.trim();
            let chord = parts.next()?.trim();
            if spoken.is_empty() || chord.is_empty() {
                return None;
            }
            Some(VoiceCommandRow {
                spoken: spoken.to_owned(),
                chord: chord.to_owned(),
            })
        })
        .collect()
}

fn persist_voice_commands(app: &mut VoiceTypingApp) {
    let body = app
        .command_entries
        .iter()
        .filter(|entry| !entry.spoken.trim().is_empty() && !entry.chord.trim().is_empty())
        .map(|entry| format!("{} => {}", entry.spoken.trim(), entry.chord.trim()))
        .collect::<Vec<_>>()
        .join("\n");

    let content = if body.is_empty() {
        String::from("# Voice command key chords\n")
    } else {
        format!("# Voice command key chords\n{body}\n")
    };

    if let Err(err) = fs::create_dir_all(app_data_dir()) {
        app.status = format!("save failed: {err}");
        return;
    }

    match fs::write(voice_commands_path(), content) {
        Ok(()) => app.status = format!("saved {} voice commands", app.command_entries.len()),
        Err(err) => app.status = format!("save failed: {err}"),
    }
}

fn active_audio_glow(app: &VoiceTypingApp) -> f32 {
    if !matches!(app.mic, MicState::Active) {
        return 0.0;
    }

    let shimmer = 0.92 + (app.phase * std::f32::consts::TAU).sin().abs() * 0.08;
    (app.audio_level * shimmer).clamp(0.0, 1.0)
}

fn capture_chord_from_event(event: &keyboard::Event) -> Option<String> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };

    if *repeat {
        return None;
    }

    let mut parts = Vec::new();
    if modifiers.control() {
        parts.push(String::from("Ctrl"));
    }
    if modifiers.alt() {
        parts.push(String::from("Alt"));
    }
    if modifiers.shift() {
        parts.push(String::from("Shift"));
    }
    if modifiers.logo() {
        parts.push(String::from("Win"));
    }

    let key_name = match key.as_ref() {
        keyboard::Key::Named(key::Named::Space) => Some(String::from("Space")),
        keyboard::Key::Named(key::Named::Enter) => Some(String::from("Enter")),
        keyboard::Key::Named(key::Named::Tab) => Some(String::from("Tab")),
        keyboard::Key::Named(key::Named::Escape) => Some(String::from("Escape")),
        keyboard::Key::Named(key::Named::Backspace) => Some(String::from("Backspace")),
        keyboard::Key::Named(key::Named::Delete) => Some(String::from("Delete")),
        keyboard::Key::Named(key::Named::ArrowLeft) => Some(String::from("Left")),
        keyboard::Key::Named(key::Named::ArrowRight) => Some(String::from("Right")),
        keyboard::Key::Named(key::Named::ArrowUp) => Some(String::from("Up")),
        keyboard::Key::Named(key::Named::ArrowDown) => Some(String::from("Down")),
        keyboard::Key::Named(key::Named::Home) => Some(String::from("Home")),
        keyboard::Key::Named(key::Named::End) => Some(String::from("End")),
        keyboard::Key::Named(key::Named::PageUp) => Some(String::from("PageUp")),
        keyboard::Key::Named(key::Named::PageDown) => Some(String::from("PageDown")),
        keyboard::Key::Character(value) => Some(value.to_ascii_uppercase()),
        _ => None,
    }?;

    if matches!(
        key.as_ref(),
        keyboard::Key::Named(
            key::Named::Control | key::Named::Shift | key::Named::Alt | key::Named::Super
        )
    ) {
        return None;
    }

    parts.push(key_name);
    Some(parts.join("+"))
}

fn find_voice_command<'a>(app: &'a VoiceTypingApp, text: &str) -> Option<&'a VoiceCommandRow> {
    let needle = text.trim().to_ascii_lowercase();
    app.command_entries.iter().find(|entry| {
        !entry.spoken.trim().is_empty() && entry.spoken.trim().eq_ignore_ascii_case(&needle)
    })
}

fn ring_color(app: &VoiceTypingApp) -> Color {
    if app
        .target_warning_until
        .is_some_and(|until| until > Instant::now())
    {
        return PURPLE;
    }

    match app.mic {
        MicState::Booting => ORANGE,
        MicState::Downloading => ORANGE,
        MicState::Active => BLUE,
        MicState::Error => RED,
        MicState::Idle => SURFACE_EDGE,
    }
}

fn pizza_glyph(progress: f32) -> &'static str {
    match (progress.clamp(0.0, 1.0) * 8.0).round() as i32 {
        i if i <= 0 => "◔",
        1 => "◔",
        2 => "◑",
        3 => "◑",
        4 => "◕",
        5 => "◕",
        6 => "⬤",
        _ => "⬤",
    }
}

fn read_text_with_legacy(path: PathBuf, legacy_name: &str) -> std::io::Result<String> {
    fs::read_to_string(&path).or_else(|_| {
        std::env::current_dir()
            .map(|dir| dir.join(legacy_name))
            .ok()
            .filter(|legacy| legacy != &path && legacy.exists())
            .map(fs::read_to_string)
            .unwrap_or_else(|| fs::read_to_string(&path))
    })
}

fn mix_color(base: Color, highlight: Color, amount: f32) -> Color {
    let t = amount.clamp(0.0, 1.0);
    Color {
        r: base.r + (highlight.r - base.r) * t,
        g: base.g + (highlight.g - base.g) * t,
        b: base.b + (highlight.b - base.b) * t,
        a: base.a + (highlight.a - base.a) * t,
    }
}

fn widget_surface() -> Color {
    if cfg!(target_os = "windows") {
        Color::from_rgba8(0, 0, 0, 0.0)
    } else if cfg!(target_os = "macos") {
        Color::from_rgba8(30, 30, 31, 0.78)
    } else {
        SURFACE
    }
}

fn current_backdrop_material(app: &VoiceTypingApp) -> BackdropMaterial {
    app.applied_backdrop
        .unwrap_or_else(|| resolve_backdrop(app.backdrop_preference))
}

fn panel_surface() -> Color {
    if cfg!(target_os = "windows") {
        Color::from_rgba8(0, 0, 0, 0.0)
    } else if cfg!(target_os = "macos") {
        Color::from_rgba8(24, 24, 26, 0.88)
    } else {
        PANEL_BG
    }
}

fn panel_inner_surface() -> Color {
    if cfg!(target_os = "windows") {
        Color::from_rgba8(0, 0, 0, 0.0)
    } else if cfg!(target_os = "macos") {
        Color::from_rgba8(18, 18, 20, 0.76)
    } else {
        Color::from_rgb8(18, 18, 20)
    }
}
