use iced::widget::svg;
use std::sync::LazyLock;

const MIC_ICON: &[u8] = br#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
<path fill="currentColor" d="M12 4.2a2.9 2.9 0 0 0-2.9 2.9v5.1a2.9 2.9 0 1 0 5.8 0V7.1A2.9 2.9 0 0 0 12 4.2Z"/>
<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M7.8 11.8a4.2 4.2 0 0 0 8.4 0M12 16v3M9.4 19h5.2"/>
</svg>"#;
const SETTINGS_ICON: &[u8] = br#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M12 8.6A3.4 3.4 0 1 0 12 15.4A3.4 3.4 0 1 0 12 8.6z"/>
<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M19.2 13.2v-2.4l-2-.7a5.8 5.8 0 0 0-.6-1.3l.9-1.9l-1.7-1.7l-1.9.9c-.4-.2-.8-.4-1.3-.6l-.7-2H9.6l-.7 2c-.5.1-.9.3-1.3.6l-1.9-.9l-1.7 1.7l.9 1.9c-.3.4-.5.8-.6 1.3l-2 .7v2.4l2 .7c.1.5.3.9.6 1.3l-.9 1.9l1.7 1.7l1.9-.9c.4.3.8.5 1.3.6l.7 2h2.4l.7-2c.5-.1.9-.3 1.3-.6l1.9.9l1.7-1.7l-.9-1.9c.3-.4.5-.8.6-1.3l2-.7z"/>
</svg>"#;
const CLOSE_ICON: &[u8] = br#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
<path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M7 7l10 10M17 7L7 17"/>
</svg>"#;
const DRAG_ICON: &[u8] = br#"<svg viewBox="0 0 24 8" xmlns="http://www.w3.org/2000/svg">
<circle cx="6" cy="4" r="1.1" fill="currentColor"/><circle cx="12" cy="4" r="1.1" fill="currentColor"/><circle cx="18" cy="4" r="1.1" fill="currentColor"/>
</svg>"#;
const HELP_ICON: &[u8] = br#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
<circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" stroke-width="1.8"/>
<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M9.9 9.3a2.4 2.4 0 1 1 3.8 1.8c-1 .7-1.5 1.2-1.5 2.1"/>
<circle cx="12" cy="17" r="1.1" fill="currentColor"/>
</svg>"#;

#[derive(Debug, Clone, Copy)]
pub enum AppIcon {
    Mic,
    Settings,
    Close,
    DragHandle,
    Help,
}

pub fn handle(icon: AppIcon) -> svg::Handle {
    match icon {
        AppIcon::Mic => MIC.clone(),
        AppIcon::Settings => SETTINGS.clone(),
        AppIcon::Close => CLOSE.clone(),
        AppIcon::DragHandle => DRAG.clone(),
        AppIcon::Help => HELP.clone(),
    }
}

static MIC: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(MIC_ICON));
static SETTINGS: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(SETTINGS_ICON));
static CLOSE: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(CLOSE_ICON));
static DRAG: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(DRAG_ICON));
static HELP: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(HELP_ICON));
