use crate::icons::{AppIcon, handle as icon_handle};
use daydream_asr::{CurrentWiredModel, DesktopSherpaAsrService};
use daydream_core::{AsrResult, AsrService, TechAcronymMapper};
use daydream_platform_windows::TextInjector;
use iced::widget::canvas::{self, Canvas, Frame, Path, Stroke};
use iced::widget::{Space, button, column, container, mouse_area, row, svg, text};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Rectangle, Settings, Shadow,
    Subscription, Task, Theme, time, window,
};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ── Layout ──────────────────────────────────────────────────────────────

const WIN_W: f32 = 120.0;
const WIN_H: f32 = 100.0;
const RADIUS: f32 = 14.0;

// ── Palette ─────────────────────────────────────────────────────────────

const HEADER: Color = Color::from_rgba8(31, 31, 32, 0.99);
const BODY: Color = Color::from_rgba8(39, 39, 40, 0.99);
const PILL: Color = Color::from_rgb8(160, 160, 160);
const BLUE: Color = Color::from_rgb8(41, 121, 255);
const GLYPH: Color = Color::from_rgb8(200, 200, 202);
const WHITE: Color = Color::WHITE;
const ORANGE: Color = Color::from_rgb8(255, 165, 0);
const IDLE_SUSPEND_AFTER: Duration = Duration::from_secs(30 * 60);

// ── App state ───────────────────────────────────────────────────────────

pub fn run() -> iced::Result {
    iced::application(DaydreamApp::default, update, view)
        .title(app_title)
        .theme(app_theme)
        .window(window_settings())
        .subscription(subscription)
        .settings(Settings::default())
        .run()
}

fn app_title(_: &DaydreamApp) -> String {
    String::from("DAYDREAM")
}

fn app_theme(_: &DaydreamApp) -> Theme {
    Theme::TokyoNight
}

struct DaydreamApp {
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

impl Default for DaydreamApp {
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
        }
    }
}

// ── Subscriptions ───────────────────────────────────────────────────────

fn subscription(app: &DaydreamApp) -> Subscription<Msg> {
    let mut subs = vec![
        window::open_events().map(Msg::Opened),
        time::every(Duration::from_millis(250)).map(|_| Msg::Bridge),
    ];
    if matches!(app.mic, MicState::Booting | MicState::Active) {
        subs.push(time::every(Duration::from_millis(60)).map(|_| Msg::Tick));
    }
    Subscription::batch(subs)
}

// ── Update ──────────────────────────────────────────────────────────────

fn update(app: &mut DaydreamApp, msg: Msg) -> Task<Msg> {
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

fn view(app: &DaydreamApp) -> Element<'_, Msg> {
    let header = mouse_area(
        container(
            row![
                Space::new().width(Length::Fixed(18.0)),
                Space::new().width(Length::Fill),
                pill(),
                Space::new().width(Length::Fill),
                button(text("×").size(14).color(GLYPH))
                    .on_press(Msg::Close)
                    .width(18)
                    .height(18)
                    .style(|_, _| transparent_btn()),
            ]
            .align_y(Alignment::Center)
            .padding([4, 8]),
        )
        .width(Length::Fill)
        .style(|_| zone_style(HEADER, true)),
    )
    .on_press(Msg::Drag);

    let body = container(mic_btn(app))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| zone_style(BODY, false));

    container(column![header, body])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| shell_style())
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

fn mic_btn(app: &DaydreamApp) -> Element<'_, Msg> {
    let (mic_color, bg_color, border_color) = match app.mic {
        MicState::Active => (WHITE, BLUE, BLUE),
        MicState::Booting => (GLYPH, Color::from_rgb8(48, 48, 49), ORANGE),
        MicState::Idle => (
            GLYPH,
            Color::from_rgb8(48, 48, 49),
            Color::from_rgb8(89, 89, 91),
        ),
    };

    button(
        svg(icon_handle(AppIcon::Mic))
            .width(Length::Fixed(26.0))
            .height(Length::Fixed(26.0))
            .style(move |_, _| svg::Style {
                color: Some(mic_color),
            }),
    )
    .on_press(Msg::Mic)
    .width(Length::Fixed(56.0))
    .height(Length::Fixed(56.0))
    .style(move |_, _| iced::widget::button::Style {
        background: Some(Background::Color(bg_color)),
        text_color: WHITE,
        border: Border {
            radius: 999.0.into(),
            width: 2.0,
            color: border_color,
        },
        shadow: Shadow::default(),
        ..Default::default()
    })
    .into()
}

// ── Styles ──────────────────────────────────────────────────────────────

fn shell_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: None,
        border: Border {
            radius: RADIUS.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.25),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

fn zone_style(color: Color, top: bool) -> iced::widget::container::Style {
    let border = if top {
        Border {
            radius: iced::border::top(RADIUS).into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        }
    } else {
        Border {
            radius: iced::border::bottom(RADIUS).into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        }
    };
    iced::widget::container::Style {
        background: Some(Background::Color(color)),
        border,
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

fn pill<'a>() -> Element<'a, Msg> {
    container(Space::new())
        .width(36)
        .height(5)
        .style(|_| iced::widget::container::Style {
            background: Some(Background::Color(PILL)),
            border: Border {
                radius: 99.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
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
            let _ = daydream_platform_windows::apply_mica_backdrop(h);
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

fn start_asr(app: &mut DaydreamApp) -> Result<(), String> {
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

fn stop_listening(app: &mut DaydreamApp) {
    if let Some(asr) = app.asr.as_mut() {
        let _ = asr.stop_real_time_session();
    }
    app.results = None;
    app.last_injected = None;
}

fn drain_results(app: &mut DaydreamApp) {
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

fn is_dup(app: &DaydreamApp, t: &str) -> bool {
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
