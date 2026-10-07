use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::config::{self, Reminder, MAX_REMINDERS};
use crate::{overlay_window, scheduler, AppState};

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
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub is_paused: bool,
    pub remaining_pause_secs: i64,
    pub remaining_reminder_secs: u64,
    pub next_reminder_name: Option<String>,
    pub reminders: Vec<ReminderStatus>,
}

fn reminder_status(rt: &scheduler::ReminderRuntime) -> ReminderStatus {
    let now = chrono::Local::now();
    let remaining_secs = rt.next_fire.and_then(|nf| {
        if nf > now {
            Some((nf - now).num_seconds().max(0) as u64)
        } else {
            Some(0)
        }
    });
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
    }
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> StatusPayload {
    let s = state.scheduler.lock().unwrap();
    let next = s.next_upcoming();
    StatusPayload {
        is_paused: s.is_paused,
        remaining_pause_secs: s.remaining_pause_secs(),
        remaining_reminder_secs: s.remaining_until_next_secs(),
        next_reminder_name: next.map(|(n, _)| n),
        reminders: s.reminders.iter().map(reminder_status).collect(),
    }
}

#[tauri::command]
pub fn list_reminders(state: State<'_, AppState>) -> Vec<ReminderStatus> {
    let s = state.scheduler.lock().unwrap();
    s.reminders.iter().map(reminder_status).collect()
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
    };

    if let Some(i) = idx {
        s.reminders[i] = runtime;
    } else {
        s.reminders.push(runtime);
    }

    let status = reminder_status(s.find(&reminder.id).unwrap());
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
            state.scheduler.lock().unwrap().active_id = None;
        }
    }
    // Destroy overlay to stop WebView2 compositing while idle.
    overlay_window::destroy(&app);
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
        });
    }
    let cfg = s.to_config();
    drop(s);
    config::save(&app, &cfg);
    let _ = app.emit("interval-changed", minutes);
    let _ = app.emit("reminders-changed", ());
}
