use iced::widget::svg;
use std::sync::LazyLock;

const MIC_ICON: &[u8] = br#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
<path fill="currentColor" d="M12 4.2a2.9 2.9 0 0 0-2.9 2.9v5.1a2.9 2.9 0 1 0 5.8 0V7.1A2.9 2.9 0 0 0 12 4.2Z"/>
<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M7.8 11.8a4.2 4.2 0 0 0 8.4 0M12 16v3M9.4 19h5.2"/>
</svg>"#;

#[derive(Debug, Clone, Copy)]
pub enum AppIcon {
    Mic,
}

pub fn handle(icon: AppIcon) -> svg::Handle {
    match icon {
        AppIcon::Mic => MIC.clone(),
    }
}

static MIC: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(MIC_ICON));
