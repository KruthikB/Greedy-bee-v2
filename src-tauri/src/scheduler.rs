use chrono::{DateTime, Duration as ChronoDuration, Local};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{
    self, Config, PausedReminderRemaining, PersistedPause, Reminder, Schedule,
};

pub struct SchedulerState {
    pub reminders: Vec<ReminderRuntime>,
    pub is_paused: bool,
    pub pause_until: Option<Instant>,
    /// Wall-clock end of a timed pause (persisted across quit).
    pub pause_until_dt: Option<DateTime<Local>>,
    pub queue: VecDeque<String>,
    pub active_id: Option<String>,
    /// When the current overlay fire started (for stuck-active recovery).
    pub active_since: Option<Instant>,
    /// True when the active overlay was started via Test (must not reschedule).
    pub active_is_test: bool,
    /// Character display height percent (aspect ratio preserved).
    pub character_size: u32,
}

pub struct ReminderRuntime {
    pub reminder: Reminder,
    pub next_fire: Option<DateTime<Local>>,
    /// Remaining time until next_fire when a pause began (preserves countdowns).
    pub paused_remaining: Option<ChronoDuration>,
}

impl SchedulerState {
    pub fn from_config(cfg: &Config) -> Self {
        let now = Local::now();
        let mut reminders: Vec<ReminderRuntime> = cfg
            .reminders
            .iter()
            .map(|r| {
                let next_fire = if r.enabled {
                    config::next_fire(&r.schedule, now)
                } else {
                    None
                };
                ReminderRuntime {
                    reminder: r.clone(),
                    next_fire,
                    paused_remaining: None,
                }
            })
            .collect();

        let mut is_paused = false;
        let mut pause_until: Option<Instant> = None;
        let mut pause_until_dt: Option<DateTime<Local>> = None;

        if let Some(pause) = &cfg.pause {
            // Restore frozen remaining deltas for each reminder.
            for entry in &pause.remaining {
                if let Some(rt) = reminders.iter_mut().find(|r| r.reminder.id == entry.id) {
                    rt.paused_remaining =
                        Some(ChronoDuration::seconds(entry.remaining_secs.max(0)));
                    rt.next_fire = None;
                }
            }

            let until_dt = pause
                .pause_until
                .as_ref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Local));

            match until_dt {
                Some(dt) if dt <= now => {
                    // Timed pause already expired while the app was closed — resume
                    // with the same remaining deltas that were frozen at pause.
                    for rt in reminders.iter_mut() {
                        if let Some(rem) = rt.paused_remaining.take() {
                            rt.next_fire = Some(now + rem);
                        } else if rt.reminder.enabled {
                            rt.next_fire = config::next_fire(&rt.reminder.schedule, now);
                        }
                    }
                    is_paused = false;
                    pause_until = None;
                    pause_until_dt = None;
                }
                other => {
                    is_paused = true;
                    pause_until_dt = other;
                    pause_until = other.map(|dt| {
                        let secs = (dt - now).num_seconds().max(0) as u64;
                        Instant::now() + Duration::from_secs(secs)
                    });
                }
            }
        }

        Self {
            reminders,
            is_paused,
            pause_until,
            pause_until_dt,
            queue: VecDeque::new(),
            active_id: None,
            active_since: None,
            active_is_test: false,
            character_size: config::clamp_character_size(cfg.character_size),
        }
    }

    pub fn remaining_pause_secs(&self) -> i64 {
        if !self.is_paused {
            return 0;
        }
        match self.pause_until {
            None => -1,
            Some(until) => {
                let now = Instant::now();
                if until > now {
                    (until - now).as_secs() as i64
                } else {
                    0
                }
            }
        }
    }

    pub fn next_upcoming(&self) -> Option<(String, DateTime<Local>)> {
        let now = Local::now();
        self.reminders
            .iter()
            .filter(|r| r.reminder.enabled)
            // Skip the reminder currently on screen (next_fire may be cleared).
            .filter(|r| self.active_id.as_deref() != Some(r.reminder.id.as_str()))
            .filter_map(|r| r.next_fire.map(|t| (r.reminder.name.clone(), t)))
            // Only future fires count toward the countdown (past = due/queued).
            .filter(|(_, t)| *t > now)
            .min_by_key(|(_, t)| *t)
    }

    pub fn remaining_until_next_secs(&self) -> u64 {
        if self.is_paused {
            return 0;
        }
        if self.active_id.is_some() {
            // Overlay is showing — don't report a bogus 0:00 from a due sibling.
            if let Some((_, next)) = self.next_upcoming() {
                let now = Local::now();
                return (next - now).num_seconds().max(0) as u64;
            }
            return 0;
        }
        let Some((_, next)) = self.next_upcoming() else {
            return 0;
        };
        let now = Local::now();
        (next - now).num_seconds().max(0) as u64
    }

    pub fn replace_reminders(&mut self, reminders: Vec<Reminder>) {
        let now = Local::now();
        self.reminders = reminders
            .into_iter()
            .map(|r| {
                let next_fire = if r.enabled {
                    config::next_fire(&r.schedule, now)
                } else {
                    None
                };
                ReminderRuntime {
                    reminder: r,
                    next_fire,
                    paused_remaining: None,
                }
            })
            .collect();
    }

    pub fn to_config(&self) -> Config {
        let pause = if self.is_paused {
            Some(PersistedPause {
                pause_until: self
                    .pause_until_dt
                    .map(|dt| dt.to_rfc3339()),
                remaining: self
                    .reminders
                    .iter()
                    .filter_map(|rt| {
                        rt.paused_remaining.map(|d| PausedReminderRemaining {
                            id: rt.reminder.id.clone(),
                            remaining_secs: d.num_seconds().max(0),
                        })
                    })
                    .collect(),
            })
        } else {
            None
        };
        Config {
            version: 2,
            reminders: self.reminders.iter().map(|r| r.reminder.clone()).collect(),
            character_size: self.character_size,
            pause,
        }
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut ReminderRuntime> {
        self.reminders.iter_mut().find(|r| r.reminder.id == id)
    }

    pub fn find(&self, id: &str) -> Option<&ReminderRuntime> {
        self.reminders.iter().find(|r| r.reminder.id == id)
    }
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PauseChangedPayload {
    pub is_paused: bool,
    pub remaining_secs: i64,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReminderFirePayload {
    pub id: String,
    pub character: String,
    pub action: String,
    pub message: String,
    pub board_text: Option<String>,
    pub not_yet_message: String,
    pub character_size: u32,
}

fn fire_payload(rt: &ReminderRuntime, character_size: u32) -> ReminderFirePayload {
    ReminderFirePayload {
        id: rt.reminder.id.clone(),
        character: rt.reminder.character.clone(),
        action: rt.reminder.action.clone(),
        message: rt.reminder.message.clone(),
        board_text: rt.reminder.board_text.clone(),
        not_yet_message: rt.reminder.not_yet_message.clone(),
        character_size,
    }
}

pub async fn run_loop(state: Arc<Mutex<SchedulerState>>, app: AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut due_ids: Vec<String> = Vec::new();
        {
            let mut s = state.lock().unwrap();
            let now_instant = Instant::now();
            let now = Local::now();

            if s.is_paused {
                if let Some(until) = s.pause_until {
                    if now_instant >= until {
                        s.is_paused = false;
                        s.pause_until = None;
                        s.pause_until_dt = None;
                        reschedule_all_after_resume(&mut s);
                        let cfg = s.to_config();
                        drop(s);
                        config::save(&app, &cfg);
                        let payload = PauseChangedPayload {
                            is_paused: false,
                            remaining_secs: 0,
                        };
                        let _ = app.emit("pause-changed", payload);
                        continue;
                    }
                }
                continue;
            }

            for rt in s.reminders.iter_mut() {
                if !rt.reminder.enabled {
                    continue;
                }
                if let Some(nf) = rt.next_fire {
                    if now >= nf {
                        due_ids.push(rt.reminder.id.clone());
                        // Push next occurrence immediately so we don't re-queue every second
                        // while waiting for the overlay. Interval waits until dismiss.
                        match &rt.reminder.schedule {
                            Schedule::Interval { .. } => {
                                // Clear until dismiss so we don't re-queue every second
                                // and don't poison the global countdown with a fake far date.
                                rt.next_fire = None;
                            }
                            Schedule::Once { .. } => {
                                rt.next_fire = None;
                            }
                            other => {
                                rt.next_fire = config::next_fire(other, now + ChronoDuration::seconds(1));
                            }
                        }
                    }
                }
            }

            for id in &due_ids {
                if !s.queue.contains(id) && s.active_id.as_deref() != Some(id.as_str()) {
                    s.queue.push_back(id.clone());
                }
            }

            // Never auto-dismiss while the user still needs to answer.
            // Only clear a stuck active_id if the overlay window itself is gone.
            if let (Some(_), Some(since)) = (&s.active_id, s.active_since) {
                if app.get_webview_window("overlay").is_none()
                    && now_instant.duration_since(since) > Duration::from_secs(45)
                {
                    eprintln!("Greedy Bee: clearing orphaned active_id (overlay window missing)");
                    s.active_id = None;
                    s.active_since = None;
                }
            }

            if s.active_id.is_none() {
                if let Some(next_id) = s.queue.pop_front() {
                    if let Some(rt) = s.find(&next_id) {
                        let payload = fire_payload(rt, s.character_size);
                        s.active_id = Some(next_id);
                        s.active_since = Some(Instant::now());
                        s.active_is_test = false;
                        drop(s);
                        fire_overlay(&app, payload);
                        continue;
                    }
                }
            }
        }
    }
}

fn reschedule_all_after_resume(s: &mut SchedulerState) {
    let now = Local::now();
    for rt in s.reminders.iter_mut() {
        if !rt.reminder.enabled {
            rt.next_fire = None;
            rt.paused_remaining = None;
            continue;
        }
        if let Some(rem) = rt.paused_remaining.take() {
            // Preserve the same time-to-fire that remained when pause began.
            rt.next_fire = Some(now + rem);
        } else if rt.next_fire.is_none()
            && s.active_id.as_deref() != Some(rt.reminder.id.as_str())
            && !s.queue.contains(&rt.reminder.id)
        {
            // No snapshot (e.g. was mid-overlay) — schedule a fresh next occurrence.
            rt.next_fire = config::next_fire(&rt.reminder.schedule, now);
        }
        // If still active/queued with next_fire None, leave it for dismiss handling.
    }
}

pub fn fire_overlay(app: &AppHandle, payload: ReminderFirePayload) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let needs_create = app.get_webview_window("overlay").is_none();
        if needs_create {
            if let Some(state) = app.try_state::<crate::AppState>() {
                if let Ok(mut ready) = state.overlay_ready.lock() {
                    *ready = false;
                }
            }
            if let Err(err) = crate::overlay_window::create(&app) {
                eprintln!("Greedy Bee: failed to create overlay: {err}");
                return;
            }
        }

        // Wait until JS has registered the listener (or timeout).
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let ready = app
                .try_state::<crate::AppState>()
                .and_then(|s| s.overlay_ready.lock().ok().map(|g| *g))
                .unwrap_or(false);
            if ready || Instant::now() >= deadline {
                if !ready {
                    eprintln!("Greedy Bee: overlay_ready timeout — emitting anyway");
                }
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }

        if let Some(overlay) = app.get_webview_window("overlay") {
            let _ = overlay.show();
            let _ = overlay.set_always_on_top(true);
            crate::platform::set_clickthrough(&overlay, true);
            let _ = overlay.emit("reminder-fire", payload);
        }
    });
}

pub fn on_dismiss(state: &Arc<Mutex<SchedulerState>>, id: &str, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    let was_test = s.active_is_test && s.active_id.as_deref() == Some(id);
    if s.active_id.as_deref() == Some(id) {
        s.active_id = None;
        s.active_since = None;
        s.active_is_test = false;
    }
    // Remove from queue if still present
    s.queue.retain(|q| q != id);

    // Test overlays must not touch schedule timers.
    if was_test {
        drop(s);
        try_fire_next(state, app);
        return;
    }

    if let Some(rt) = s.find_mut(id) {
        match &rt.reminder.schedule {
            Schedule::Interval { minutes } => {
                rt.next_fire = Some(Local::now() + ChronoDuration::minutes(*minutes as i64));
            }
            Schedule::Once { .. } => {
                rt.reminder.enabled = false;
                rt.next_fire = None;
                let cfg = s.to_config();
                drop(s);
                config::save(app, &cfg);
                try_fire_next(&Arc::clone(state), app);
                return;
            }
            other => {
                rt.next_fire = config::next_fire(other, Local::now());
            }
        }
    }
    drop(s);
    try_fire_next(state, app);
}

fn try_fire_next(state: &Arc<Mutex<SchedulerState>>, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    if s.active_id.is_some() || s.is_paused {
        return;
    }
    if let Some(next_id) = s.queue.pop_front() {
        if let Some(rt) = s.find(&next_id) {
            let payload = fire_payload(rt, s.character_size);
            s.active_id = Some(next_id);
            s.active_since = Some(Instant::now());
            s.active_is_test = false;
            drop(s);
            fire_overlay(app, payload);
        }
    }
}

pub fn pause(state: &Arc<Mutex<SchedulerState>>, duration_mins: Option<u32>, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    let now = Local::now();
    for rt in s.reminders.iter_mut() {
        if let Some(nf) = rt.next_fire {
            let rem = nf - now;
            rt.paused_remaining = Some(if rem > ChronoDuration::zero() {
                rem
            } else {
                ChronoDuration::zero()
            });
            // Clear absolute fire time while paused; resume rebuilds from snapshot.
            rt.next_fire = None;
        } else {
            // Mid-overlay / queued: keep no snapshot; dismiss will reschedule.
            rt.paused_remaining = None;
        }
    }
    s.is_paused = true;
    match duration_mins {
        Some(m) if m > 0 => {
            let secs = m as u64 * 60;
            s.pause_until = Some(Instant::now() + Duration::from_secs(secs));
            s.pause_until_dt = Some(now + ChronoDuration::seconds(secs as i64));
        }
        _ => {
            s.pause_until = None;
            s.pause_until_dt = None;
        }
    }
    let remaining = s.remaining_pause_secs();
    let cfg = s.to_config();
    drop(s);
    config::save(app, &cfg);
    let _ = app.emit(
        "pause-changed",
        PauseChangedPayload {
            is_paused: true,
            remaining_secs: remaining,
        },
    );
}

pub fn resume(state: &Arc<Mutex<SchedulerState>>, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    s.is_paused = false;
    s.pause_until = None;
    s.pause_until_dt = None;
    reschedule_all_after_resume(&mut s);
    let cfg = s.to_config();
    drop(s);
    config::save(app, &cfg);
    let _ = app.emit(
        "pause-changed",
        PauseChangedPayload {
            is_paused: false,
            remaining_secs: 0,
        },
    );
}

pub fn toggle_pause_one_hour(state: &Arc<Mutex<SchedulerState>>, app: &AppHandle) {
    let is_paused = state.lock().unwrap().is_paused;
    if is_paused {
        resume(state, app);
    } else {
        pause(state, Some(60), app);
    }
}

pub fn test_reminder(state: &Arc<Mutex<SchedulerState>>, id: Option<&str>, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    let target = if let Some(id) = id {
        s.find(id).map(|r| r.reminder.clone())
    } else {
        s.reminders
            .iter()
            .find(|r| r.reminder.enabled)
            .map(|r| r.reminder.clone())
            .or_else(|| s.reminders.first().map(|r| r.reminder.clone()))
    };
    let Some(reminder) = target else {
        return;
    };
    if s.active_id.is_some() {
        // Do not queue a test as a real reminder — that would steal the schedule.
        return;
    }
    if s.is_paused {
        return;
    }
    let size = s.character_size;
    let payload = ReminderFirePayload {
        id: reminder.id.clone(),
        character: reminder.character.clone(),
        action: reminder.action.clone(),
        message: reminder.message.clone(),
        board_text: reminder.board_text.clone(),
        not_yet_message: reminder.not_yet_message.clone(),
        character_size: size,
    };
    // Leave next_fire untouched so Test never resets timers.
    s.active_id = Some(reminder.id);
    s.active_since = Some(Instant::now());
    s.active_is_test = true;
    drop(s);
    fire_overlay(app, payload);
}
