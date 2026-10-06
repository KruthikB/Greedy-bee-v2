pub enum PlatformEvent {
    ScreenOff,
    ScreenOn,
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{get_work_area, set_clickthrough, show_error, start_monitor};

// Stubs for non-Windows (macOS/Linux) — extend later
#[cfg(not(target_os = "windows"))]
pub fn get_work_area() -> (u32, u32, i32, i32) {
    (1920, 1040, 0, 0) // safe fallback — Tauri will clip to actual screen
}

#[cfg(not(target_os = "windows"))]
pub fn set_clickthrough(_window: &tauri::WebviewWindow, _enabled: bool) {}

#[cfg(not(target_os = "windows"))]
pub fn start_monitor(_tx: tokio::sync::mpsc::UnboundedSender<PlatformEvent>) {}
