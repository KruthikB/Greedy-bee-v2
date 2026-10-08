use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::config::{self, Reminder, MAX_REMINDERS};
use crate::{scheduler, AppState};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderStatus {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub character: String,
    pub action: String,
    pub message: String,
    pub board_text: Option<String>,
    pub not_yet_message: String,
    pub schedule: config::Schedule,
    pub schedule_summary: String,
    pub next_fire: Option<String>,
    pub remaining_secs: Option<u64>,
    pub queued: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub is_paused: bool,
    pub remaining_pause_secs: i64,
    pub remaining_reminder_secs: u64,
    pub next_reminder_name: Option<String>,
    pub active_reminder_id: Option<String>,
    pub character_size: u32,
    pub reminders: Vec<ReminderStatus>,
}

fn reminder_status(
    rt: &scheduler::ReminderRuntime,
    is_paused: bool,
    queued: bool,
) -> ReminderStatus {
    let now = chrono::Local::now();
    // While paused, report the frozen snapshot so countdowns do not keep ticking.
    let remaining_secs = if queued {
        None
    } else if is_paused {
        rt.paused_remaining
            .map(|d| d.num_seconds().max(0) as u64)
            .or_else(|| {
                rt.next_fire.map(|nf| {
                    if nf > now {
                        (nf - now).num_seconds().max(0) as u64
                    } else {
                        0
                    }
                })
            })
    } else {
        rt.next_fire.map(|nf| {
            if nf > now {
                (nf - now).num_seconds().max(0) as u64
            } else {
                0
            }
        })
    };
    ReminderStatus {
        id: rt.reminder.id.clone(),
        name: rt.reminder.name.clone(),
        enabled: rt.reminder.enabled,
        character: rt.reminder.character.clone(),
        action: rt.reminder.action.clone(),
        message: rt.reminder.message.clone(),
        board_text: rt.reminder.board_text.clone(),
        not_yet_message: rt.reminder.not_yet_message.clone(),
        schedule: rt.reminder.schedule.clone(),
        schedule_summary: config::schedule_summary(&rt.reminder.schedule),
        next_fire: config::format_next_fire(rt.next_fire),
        remaining_secs,
        queued,
    }
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> StatusPayload {
    let s = state.scheduler.lock().unwrap();
    let next = s.next_upcoming();
    let is_paused = s.is_paused;
    StatusPayload {
        is_paused,
        remaining_pause_secs: s.remaining_pause_secs(),
        remaining_reminder_secs: s.remaining_until_next_secs(),
        next_reminder_name: next.map(|(n, _)| n),
        active_reminder_id: s.active_id.clone(),
        character_size: s.character_size,
        reminders: s
            .reminders
            .iter()
            .map(|r| reminder_status(r, is_paused, s.queue.contains(&r.reminder.id)))
            .collect(),
    }
}

#[tauri::command]
pub fn list_reminders(state: State<'_, AppState>) -> Vec<ReminderStatus> {
    let s = state.scheduler.lock().unwrap();
    let is_paused = s.is_paused;
    s.reminders
        .iter()
        .map(|r| reminder_status(r, is_paused, s.queue.contains(&r.reminder.id)))
        .collect()
}

#[tauri::command]
pub fn set_character_size(
    size: u32,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<u32, String> {
    let size = config::clamp_character_size(size);
    let mut s = state.scheduler.lock().unwrap();
    s.character_size = size;
    let cfg = s.to_config();
    drop(s);
    config::save(&app, &cfg);
    let _ = app.emit("character-size-changed", size);
    Ok(size)
}

#[tauri::command]
pub fn save_reminder(
    reminder: Reminder,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ReminderStatus, String> {
    config::validate_reminder(&reminder)?;

    let mut s = state.scheduler.lock().unwrap();
    let idx = s
        .reminders
        .iter()
        .position(|r| r.reminder.id == reminder.id);

    if idx.is_none() && s.reminders.len() >= MAX_REMINDERS {
        return Err(format!("Maximum of {MAX_REMINDERS} reminders allowed"));
    }

    let mut reminder = reminder;
    if reminder.id.trim().is_empty() {
        reminder.id = Uuid::new_v4().to_string();
    }

    let now = chrono::Local::now();
    let next_fire = if reminder.enabled {
        config::next_fire(&reminder.schedule, now)
    } else {
        None
    };

    let runtime = scheduler::ReminderRuntime {
        reminder: reminder.clone(),
        next_fire,
        paused_remaining: None,
    };

    if let Some(i) = idx {
        s.reminders[i] = runtime;
    } else {
        s.reminders.push(runtime);
    }

    let queued = s.queue.contains(&reminder.id);
    let status = reminder_status(s.find(&reminder.id).unwrap(), s.is_paused, queued);
    let cfg = s.to_config();
    drop(s);
    config::save(&app, &cfg);
    let _ = app.emit("reminders-changed", ());
    Ok(status)
}

#[tauri::command]
pub fn delete_reminder(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let mut s = state.scheduler.lock().unwrap();
    let before = s.reminders.len();
    s.reminders.retain(|r| r.reminder.id != id);
    if s.reminders.len() == before {
        return Err("Reminder not found".into());
    }
    s.queue.retain(|q| q != &id);
    if s.active_id.as_deref() == Some(id.as_str()) {
        s.active_id = None;
        s.active_since = None;
    }
    let cfg = s.to_config();
    drop(s);
    config::save(&app, &cfg);
    let _ = app.emit("reminders-changed", ());
    Ok(())
}

#[tauri::command]
pub fn set_reminder_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut s = state.scheduler.lock().unwrap();
    let rt = s.find_mut(&id).ok_or_else(|| "Reminder not found".to_string())?;
    rt.reminder.enabled = enabled;
    rt.next_fire = if enabled {
        config::next_fire(&rt.reminder.schedule, chrono::Local::now())
    } else {
        None
    };
    let cfg = s.to_config();
    drop(s);
    config::save(&app, &cfg);
    let _ = app.emit("reminders-changed", ());
    Ok(())
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
pub fn test_reminder(id: Option<String>, state: State<'_, AppState>, app: AppHandle) {
    scheduler::test_reminder(&state.scheduler, id.as_deref(), &app);
}

#[tauri::command]
pub fn dismiss_overlay(id: Option<String>, state: State<'_, AppState>, app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    if let Some(id) = id {
        scheduler::on_dismiss(&state.scheduler, &id, &app);
    } else {
        let active = state.scheduler.lock().unwrap().active_id.clone();
        if let Some(id) = active {
            scheduler::on_dismiss(&state.scheduler, &id, &app);
        } else {
            let mut s = state.scheduler.lock().unwrap();
            s.active_id = None;
            s.active_since = None;
        }
    }
    // Keep the overlay WebView alive so the next fire does not race cold start.
    let _ = app.emit("overlay-dismissed", ());
}

#[tauri::command]
pub fn overlay_ready(state: State<'_, AppState>) {
    if let Ok(mut ready) = state.overlay_ready.lock() {
        *ready = true;
    }
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

/// Kept for older settings UI during migration; creates/updates the first interval reminder.
#[tauri::command]
pub fn set_interval(minutes: u32, state: State<'_, AppState>, app: AppHandle) {
    let minutes = minutes.clamp(1, 240);
    let mut s = state.scheduler.lock().unwrap();
    if let Some(rt) = s.reminders.iter_mut().find(|r| {
        matches!(r.reminder.schedule, config::Schedule::Interval { .. })
    }) {
        rt.reminder.schedule = config::Schedule::Interval { minutes };
        if rt.reminder.enabled {
            rt.next_fire = config::next_fire(&rt.reminder.schedule, chrono::Local::now());
        }
    } else if s.reminders.len() < MAX_REMINDERS {
        let r = config::default_water_reminder(minutes);
        let next = config::next_fire(&r.schedule, chrono::Local::now());
        s.reminders.push(scheduler::ReminderRuntime {
            reminder: r,
            next_fire: next,
            paused_remaining: None,
        });
    }
    let cfg = s.to_config();
    drop(s);
    config::save(&app, &cfg);
    let _ = app.emit("interval-changed", minutes);
    let _ = app.emit("reminders-changed", ());
}
