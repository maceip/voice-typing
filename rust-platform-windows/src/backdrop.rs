use anyhow::{Context, Result, bail};

#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::{
    DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DwmSetWindowAttribute,
};

#[cfg(target_os = "windows")]
pub fn apply_mica_backdrop(window: &dyn HasWindowHandle) -> Result<()> {
    let handle = window
        .window_handle()
        .context("failed to access native window handle")?;

    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        bail!("native window is not a Win32 HWND");
    };

    let hwnd = HWND(handle.hwnd.get() as _);
    let backdrop = DWMSBT_MAINWINDOW;

    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            std::mem::size_of_val(&backdrop) as u32,
        )
    }
    .context("failed to apply Mica backdrop")?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn apply_mica_backdrop(_window: &dyn std::any::Any) -> Result<()> {
    Ok(())
}
