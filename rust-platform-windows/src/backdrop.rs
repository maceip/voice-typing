use anyhow::{Context, Result, bail};

#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::{
    DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DwmSetWindowAttribute,
};
#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackdropPreference {
    Light,
    Dark,
    FollowSystem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackdropMaterial {
    Acrylic,
    Mica,
}

#[cfg(target_os = "windows")]
pub fn resolve_backdrop(preference: BackdropPreference) -> BackdropMaterial {
    match preference {
        BackdropPreference::Light => BackdropMaterial::Acrylic,
        BackdropPreference::Dark => BackdropMaterial::Mica,
        BackdropPreference::FollowSystem => {
            if system_prefers_light() {
                BackdropMaterial::Acrylic
            } else {
                BackdropMaterial::Mica
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn apply_backdrop(window: &dyn HasWindowHandle, material: BackdropMaterial) -> Result<()> {
    let handle = window
        .window_handle()
        .context("failed to access native window handle")?;

    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        bail!("native window is not a Win32 HWND");
    };

    let hwnd = HWND(handle.hwnd.get() as _);
    apply_backdrop_to_hwnd(hwnd, material)
}

#[cfg(target_os = "windows")]
pub fn apply_backdrop_to_hwnd(hwnd: HWND, material: BackdropMaterial) -> Result<()> {
    let backdrop = match material {
        BackdropMaterial::Acrylic => DWMSBT_TRANSIENTWINDOW.0,
        BackdropMaterial::Mica => DWMSBT_TABBEDWINDOW.0,
    };

    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            std::mem::size_of::<i32>() as u32,
        )
    }
    .with_context(|| format!("failed to apply Windows backdrop {:?}", material))?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn system_prefers_light() -> bool {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let Ok(key) = key else {
        return false;
    };

    key.get_value::<u32, _>("AppsUseLightTheme").unwrap_or(0) != 0
}

#[cfg(not(target_os = "windows"))]
pub fn resolve_backdrop(preference: BackdropPreference) -> BackdropMaterial {
    match preference {
        BackdropPreference::Light => BackdropMaterial::Acrylic,
        BackdropPreference::Dark | BackdropPreference::FollowSystem => BackdropMaterial::Mica,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_backdrop(_window: &dyn std::any::Any, _material: BackdropMaterial) -> Result<()> {
    Ok(())
}
