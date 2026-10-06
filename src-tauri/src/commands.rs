use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

use crate::{config, scheduler, AppState};

#[derive(serde::Serialize)]
pub struct StatusPayload {
    is_paused: bool,
    interval_minutes: u32,
    remaining_pause_secs: i64,
    remaining_reminder_secs: u64,
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> StatusPayload {
    let s = state.scheduler.lock().unwrap();
    StatusPayload {
        is_paused: s.is_paused,
        interval_minutes: s.interval_minutes,
        remaining_pause_secs: s.remaining_pause_secs(),
        remaining_reminder_secs: s.remaining_reminder_secs(),
    }
}

#[tauri::command]
pub fn set_interval(minutes: u32, state: State<'_, AppState>, app: AppHandle) {
    {
        let mut s = state.scheduler.lock().unwrap();
        s.interval_minutes = minutes;
        if !s.is_paused {
            s.next_reminder_at = Instant::now() + Duration::from_secs(minutes as u64 * 60);
        }
    }
    let cfg = config::Config {
        reminder_interval_minutes: minutes,
    };
    config::save(&app, &cfg);
    let _ = app.emit("interval-changed", minutes);
}

#[tauri::command]
pub fn pause_reminders(
    duration_minutes: Option<u32>,
    state: State<'_, AppState>,
    app: AppHandle,
) {
    scheduler::pause(&state.scheduler, duration_minutes, &app);
}

#[tauri::command]
pub fn resume_reminders(state: State<'_, AppState>, app: AppHandle) {
    scheduler::resume(&state.scheduler, &app);
}

#[tauri::command]
pub fn test_reminder(app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.show();
        let _ = overlay.set_always_on_top(true);
        let _ = overlay.emit("reminder-fire", ());
    }
}

#[tauri::command]
pub fn dismiss_overlay(state: State<'_, AppState>, app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    // Reset the countdown after user acknowledges
    let mut s = state.scheduler.lock().unwrap();
    if !s.is_paused {
        s.next_reminder_at =
            Instant::now() + Duration::from_secs(s.interval_minutes as u64 * 60);
    }
    drop(s);
    let _ = app.emit("overlay-dismissed", ());
}

#[tauri::command]
pub fn set_overlay_clickthrough(enabled: bool, app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        crate::platform::set_clickthrough(&overlay, enabled);
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
