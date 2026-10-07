use chrono::{DateTime, Duration as ChronoDuration, Local};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{self, Config, Reminder};

pub struct SchedulerState {
    pub reminders: Vec<ReminderRuntime>,
    pub is_paused: bool,
    pub pause_until: Option<Instant>,
    pub queue: VecDeque<String>,
    pub active_id: Option<String>,
}

pub struct ReminderRuntime {
    pub reminder: Reminder,
    pub next_fire: Option<DateTime<Local>>,
}

impl SchedulerState {
    pub fn from_config(cfg: &Config) -> Self {
        let now = Local::now();
        let reminders = cfg
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
                }
            })
            .collect();
        Self {
            reminders,
            is_paused: false,
            pause_until: None,
            queue: VecDeque::new(),
            active_id: None,
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
        self.reminders
            .iter()
            .filter(|r| r.reminder.enabled)
            .filter_map(|r| r.next_fire.map(|t| (r.reminder.name.clone(), t)))
            .min_by_key(|(_, t)| *t)
    }

    pub fn remaining_until_next_secs(&self) -> u64 {
        if self.is_paused {
            return 0;
        }
        let Some((_, next)) = self.next_upcoming() else {
            return 0;
        };
        let now = Local::now();
        if next > now {
            (next - now).num_seconds().max(0) as u64
        } else {
            0
        }
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
                }
            })
            .collect();
    }

    pub fn to_config(&self) -> Config {
        Config {
            version: 2,
            reminders: self.reminders.iter().map(|r| r.reminder.clone()).collect(),
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
                        reschedule_all_after_resume(&mut s);
                        let payload = PauseChangedPayload {
                            is_paused: false,
                            remaining_secs: 0,
                        };
                        let _ = app.emit("pause-changed", payload);
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
                                // Keep next_fire in the past until dismissed; mark with None
                                // temporarily by setting far future after enqueue below.
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
                if let Some(rt) = s.find_mut(id) {
                    if matches!(rt.reminder.schedule, Schedule::Interval { .. }) {
                        // Park until dismiss so we don't re-fire every second.
                        rt.next_fire = Some(now + ChronoDuration::days(3650));
                    }
                }
                if !s.queue.contains(id) && s.active_id.as_deref() != Some(id.as_str()) {
                    s.queue.push_back(id.clone());
                }
            }

            if s.active_id.is_none() {
                if let Some(next_id) = s.queue.pop_front() {
                    if let Some(rt) = s.find(&next_id) {
                        let payload = ReminderFirePayload {
                            id: rt.reminder.id.clone(),
                            character: rt.reminder.character.clone(),
                            action: rt.reminder.action.clone(),
                            message: rt.reminder.message.clone(),
                            board_text: rt.reminder.board_text.clone(),
                            not_yet_message: rt.reminder.not_yet_message.clone(),
                        };
                        s.active_id = Some(next_id);
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
            continue;
        }
        // Missed once/daily/weekly → next occurrence; interval → restart countdown.
        rt.next_fire = config::next_fire(&rt.reminder.schedule, now);
    }
}

pub fn fire_overlay(app: &AppHandle, payload: ReminderFirePayload) {
    // Ensure overlay window exists (on-demand lifecycle).
    if app.get_webview_window("overlay").is_none() {
        if let Err(err) = crate::overlay_window::create(app) {
            eprintln!("Greedy Bee: failed to create overlay: {err}");
            return;
        }
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.show();
        let _ = overlay.set_always_on_top(true);
        let _ = overlay.emit("reminder-fire", payload);
    }
}

pub fn on_dismiss(state: &Arc<Mutex<SchedulerState>>, id: &str, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    if s.active_id.as_deref() == Some(id) {
        s.active_id = None;
    }
    // Remove from queue if still present
    s.queue.retain(|q| q != id);

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
                // Try to show next queued
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
            let payload = ReminderFirePayload {
                id: rt.reminder.id.clone(),
                character: rt.reminder.character.clone(),
                action: rt.reminder.action.clone(),
                message: rt.reminder.message.clone(),
                board_text: rt.reminder.board_text.clone(),
                not_yet_message: rt.reminder.not_yet_message.clone(),
            };
            s.active_id = Some(next_id);
            drop(s);
            fire_overlay(app, payload);
        }
    }
}

pub fn pause(state: &Arc<Mutex<SchedulerState>>, duration_mins: Option<u32>, app: &AppHandle) {
    let mut s = state.lock().unwrap();
    s.is_paused = true;
    s.pause_until = duration_mins.map(|m| Instant::now() + Duration::from_secs(m as u64 * 60));
    let remaining = s.remaining_pause_secs();
    drop(s);
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
    reschedule_all_after_resume(&mut s);
    drop(s);
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
        if !s.queue.contains(&reminder.id) {
            s.queue.push_back(reminder.id.clone());
        }
        return;
    }
    let payload = ReminderFirePayload {
        id: reminder.id.clone(),
        character: reminder.character.clone(),
        action: reminder.action.clone(),
        message: reminder.message.clone(),
        board_text: reminder.board_text.clone(),
        not_yet_message: reminder.not_yet_message.clone(),
    };
    s.active_id = Some(reminder.id);
    drop(s);
    fire_overlay(app, payload);
}
