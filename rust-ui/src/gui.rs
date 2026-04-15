use crate::icons::{AppIcon, handle as icon_handle};
use voice_typing_asr::{CurrentWiredModel, DesktopSherpaAsrService};
use voice_typing_core::{AsrResult, AsrService, TechAcronymMapper};
use voice_typing_platform_windows::TextInjector;
use iced::widget::canvas::{self, Canvas, Frame, Path, Stroke};
use iced::widget::{Space, button, column, container, mouse_area, row, svg, text};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Rectangle, Settings, Shadow,
    Subscription, Task, Theme, time, window,
};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ── Layout ──────────────────────────────────────────────────────────────

const WIN_W: f32 = 142.0;
const WIN_H: f32 = 34.0;
const RADIUS: f32 = 12.0;

// ── Palette ─────────────────────────────────────────────────────────────

const SURFACE: Color = Color::from_rgba8(30, 30, 31, 0.99);
const SURFACE_EDGE: Color = Color::from_rgb8(36, 36, 38);
const HANDLE: Color = Color::from_rgb8(214, 214, 216);
const BLUE: Color = Color::from_rgb8(41, 121, 255);
const GLYPH: Color = Color::from_rgb8(200, 200, 202);
const WHITE: Color = Color::WHITE;
const ORANGE: Color = Color::from_rgb8(255, 165, 0);
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MicState {
    Idle,
    Booting,
    Active,
}

#[derive(Debug, Clone)]
enum Msg {
    Opened(window::Id),
    Mic,
    Config,
    Help,
    Drag,
    Close,
    Tick,
    Bridge,
}

impl Default for VoiceTypingApp {
    fn default() -> Self {
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
        }
    }
}

// ── Subscriptions ───────────────────────────────────────────────────────

fn subscription(app: &VoiceTypingApp) -> Subscription<Msg> {
    let mut subs = vec![
        window::open_events().map(Msg::Opened),
        time::every(Duration::from_millis(250)).map(|_| Msg::Bridge),
    ];
    if matches!(app.mic, MicState::Booting | MicState::Active) {
        subs.push(time::every(Duration::from_millis(30)).map(|_| Msg::Tick));
    }
    Subscription::batch(subs)
}

// ── Update ──────────────────────────────────────────────────────────────

fn update(app: &mut VoiceTypingApp, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::Opened(id) => {
            app.window_id = Some(id);
            return configure_window(id);
        }
        Msg::Mic => {
            if matches!(app.mic, MicState::Active) {
                stop_listening(app);
                app.mic = MicState::Idle;
                app.status = String::from("idle");
                if let Some(b) = crate::bridge::get() {
                    b.set_state("idle");
                }
            } else {
                app.mic = MicState::Booting;
                app.status = String::from("booting");
                app.last_injected = None;
                app.last_activity = Instant::now();
                app.last_recovery_attempt = None;
                if let Some(b) = crate::bridge::get() {
                    b.set_state("booting");
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
                        app.mic = MicState::Idle;
                        app.status = e;
                        if let Some(b) = crate::bridge::get() {
                            b.set_state("idle");
                        }
                    }
                }
            }
        }
        Msg::Config => {
            app.status = String::from("settings later");
        }
        Msg::Help => {
            app.status = String::from("help later");
        }
        Msg::Drag => {
            if let Some(id) = app.window_id {
                return window::drag(id);
            }
        }
        Msg::Close => {
            stop_listening(app);
            return iced::exit();
        }
        Msg::Tick => {
            app.phase = (app.phase + 0.055) % 1.0;
            if matches!(app.mic, MicState::Active)
                && app.last_activity.elapsed() >= IDLE_SUSPEND_AFTER
            {
                stop_listening(app);
                app.mic = MicState::Idle;
                app.status = String::from("pipeline suspended after inactivity");
                if let Some(b) = crate::bridge::get() {
                    b.set_state("idle");
                }
                return Task::none();
            }
            drain_results(app);
            maybe_recover_session(app);
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
    }
    Task::none()
}

// ── View ────────────────────────────────────────────────────────────────

fn view(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let chrome = row![
        container(left_handle())
            .width(Length::Fixed(12.0))
            .height(Length::Fixed(20.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        container(mic_btn(app))
            .width(Length::Fill)
            .height(Length::Fixed(20.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        container(trailing_menu())
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(20.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .padding([6, 8]);

    mouse_area(
        container(chrome)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| shell_style()),
    )
    .on_press(Msg::Drag)
    .into()
}

// ── SVG icon button ─────────────────────────────────────────────────────

fn svg_button(
    icon: AppIcon,
    size: f32,
    color: Color,
    msg: Msg,
) -> iced::widget::Button<'static, Msg> {
    button(
        svg(icon_handle(icon))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(move |_, _| svg::Style { color: Some(color) }),
    )
    .on_press(msg)
    .style(|_, _| transparent_btn())
}

// ── Mic button (styled round button with SVG icon) ─────────────────────

fn mic_btn(app: &VoiceTypingApp) -> Element<'_, Msg> {
    let mic_color = match app.mic {
        MicState::Active => BLUE,
        MicState::Booting => ORANGE,
        MicState::Idle => WHITE,
    };

    button(
        svg(icon_handle(AppIcon::Mic))
            .width(Length::Fixed(17.0))
            .height(Length::Fixed(17.0))
            .style(move |_, _| svg::Style {
                color: Some(mic_color),
            }),
    )
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

fn shell_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(SURFACE)),
        border: Border {
            radius: RADIUS.into(),
            width: 1.0,
            color: SURFACE_EDGE,
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.18),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

fn mic_btn_style() -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: WHITE,
        border: Border {
            radius: 999.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        ..Default::default()
    }
}

fn transparent_btn() -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: WHITE,
        border: Border {
            radius: 8.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
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
        text("…")
            .size(17)
            .line_height(iced::widget::text::LineHeight::Relative(1.0))
            .color(GLYPH),
    )
        .width(Length::Fixed(14.0))
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

// ── Window setup ────────────────────────────────────────────────────────

fn window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(WIN_W, WIN_H),
        min_size: Some(iced::Size::new(WIN_W, WIN_H)),
        max_size: Some(iced::Size::new(WIN_W, WIN_H)),
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

fn configure_window(id: window::Id) -> Task<Msg> {
    #[cfg(target_os = "windows")]
    {
        return window::run(id, |h| {
            let _ = voice_typing_platform_windows::apply_mica_backdrop(h);
        })
        .discard();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = id;
        Task::none()
    }
}

// ── ASR helpers ─────────────────────────────────────────────────────────

fn start_asr(app: &mut VoiceTypingApp) -> Result<(), String> {
    if app.asr.is_none() {
        let mut asr = DesktopSherpaAsrService::new();
        asr.initialize_blocking(CurrentWiredModel::MODEL_DIR)
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
            app.mic = MicState::Idle;
            app.status = format!("mic recovery failed: {err}");
            if let Some(b) = crate::bridge::get() {
                b.set_state("idle");
            }
        }
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
        app.status = format!("sent {}", clock());
        app.last_injected = Some((mapped.clone(), Instant::now()));
        app.last_activity = Instant::now();
        if let Some(b) = crate::bridge::get() {
            b.send_transcript(&mapped, r.is_final);
        }
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

fn user_dictionary_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("user_dictionary.txt")
}
