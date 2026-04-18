use anyhow::{Context, Result, anyhow, bail};

#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::{
    DWM_BB_ENABLE, DWM_BLURBEHIND, DWMNCRENDERINGPOLICY, DWMNCRP_DISABLED, DWMSBT_TABBEDWINDOW,
    DWMWA_NCRENDERING_POLICY, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DwmEnableBlurBehindWindow, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::MARGINS;
#[cfg(target_os = "windows")]
use windows::core::BOOL;
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
        .map_err(|err| anyhow!("failed to access native window handle: {err}"))?;

    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        bail!("native window is not a Win32 HWND");
    };

    let hwnd = HWND(handle.hwnd.get() as _);
    apply_backdrop_to_hwnd(hwnd, material)
}

#[cfg(target_os = "windows")]
pub fn apply_backdrop_to_hwnd(hwnd: HWND, material: BackdropMaterial) -> Result<()> {
    let backdrop = match material {
        BackdropMaterial::Acrylic => 1i32,
        BackdropMaterial::Mica => DWMSBT_TABBEDWINDOW.0,
    };
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    let blur = DWM_BLURBEHIND {
        dwFlags: DWM_BB_ENABLE,
        fEnable: BOOL::from(true),
        ..Default::default()
    };
    let is_dark_mode = BOOL::from(matches!(material, BackdropMaterial::Mica));
    let nc_policy: DWMNCRENDERINGPOLICY = DWMNCRP_DISABLED;

    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &margins)
            .context("failed to extend acrylic frame into the client area")?;
        DwmEnableBlurBehindWindow(hwnd, &blur)
            .context("failed to enable window blur behind the client area")?;

        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &nc_policy as *const DWMNCRENDERINGPOLICY as *const _,
            std::mem::size_of::<DWMNCRENDERINGPOLICY>() as u32,
        );

        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &is_dark_mode as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );

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
