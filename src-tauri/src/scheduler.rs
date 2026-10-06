use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

pub struct SchedulerState {
    pub interval_minutes: u32,
    pub is_paused: bool,
    pub pause_until: Option<Instant>, // None while paused = indefinite
    pub next_reminder_at: Instant,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            interval_minutes: 15,
            is_paused: false,
            pause_until: None,
            next_reminder_at: Instant::now() + Duration::from_secs(15 * 60),
        }
    }

    pub fn remaining_pause_secs(&self) -> i64 {
        if !self.is_paused {
            return 0;
        }
        match self.pause_until {
            None => -1, // indefinite
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

    pub fn remaining_reminder_secs(&self) -> u64 {
        if self.is_paused {
            return 0;
        }
        let now = Instant::now();
        if self.next_reminder_at > now {
            (self.next_reminder_at - now).as_secs()
        } else {
            0
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct PauseChangedPayload {
    pub is_paused: bool,
    pub remaining_secs: i64,
}

pub async fn run_loop(state: Arc<Mutex<SchedulerState>>, app: AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut s = state.lock().unwrap();
        let now = Instant::now();

        // Auto-resume a timed pause
        if s.is_paused {
            if let Some(until) = s.pause_until {
                if now >= until {
                    s.is_paused = false;
                    s.pause_until = None;
                    s.next_reminder_at =
                        now + Duration::from_secs(s.interval_minutes as u64 * 60);
                    let payload = PauseChangedPayload {
                        is_paused: false,
                        remaining_secs: 0,
                    };
                    let _ = app.emit("pause-changed", payload);
                }
            }
            continue;
        }

        // Fire reminder
        if now >= s.next_reminder_at {
            s.next_reminder_at =
                now + Duration::from_secs(s.interval_minutes as u64 * 60);
            drop(s); // release lock before touching windows

            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.show();
                let _ = overlay.set_always_on_top(true);
                let _ = overlay.emit("reminder-fire", ());
            }
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
    s.next_reminder_at = Instant::now() + Duration::from_secs(s.interval_minutes as u64 * 60);
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
