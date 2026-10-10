use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveTime, Timelike, Weekday};
// Timelike used by format_next_fire
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;
use uuid::Uuid;

pub const MAX_REMINDERS: usize = 10;
pub const MAX_MESSAGE_LEN: usize = 80;
pub const MAX_BOARD_TEXT_LEN: usize = 30;
pub const MIN_BOARD_TEXT_LEN: usize = 20;
pub const MAX_BOARD_LINK_LEN: usize = 100;
pub const MAX_NAME_LEN: usize = 60;

fn is_board_link(s: &str) -> bool {
    let t = s.trim().to_ascii_lowercase();
    t.starts_with("http://") || t.starts_with("https://")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub reminders: Vec<Reminder>,
    /// Display height of the character as a percent of the overlay slot (30–100).
    /// Aspect ratio is always preserved.
    #[serde(default = "default_character_size")]
    pub character_size: u32,
    /// Persisted pause snapshot so timers survive a full quit/relaunch.
    #[serde(default)]
    pub pause: Option<PersistedPause>,
}

/// Frozen countdown state while reminders are paused.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPause {
    /// Absolute local time when a timed pause ends. `None` = paused indefinitely.
    pub pause_until: Option<String>,
    /// Remaining seconds until each reminder's next fire (frozen at pause).
    #[serde(default)]
    pub remaining: Vec<PausedReminderRemaining>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PausedReminderRemaining {
    pub id: String,
    pub remaining_secs: i64,
}

fn default_version() -> u32 {
    2
}

fn default_character_size() -> u32 {
    55
}

pub const MIN_CHARACTER_SIZE: u32 = 30;
pub const MAX_CHARACTER_SIZE: u32 = 100;

pub fn clamp_character_size(size: u32) -> u32 {
    size.clamp(MIN_CHARACTER_SIZE, MAX_CHARACTER_SIZE)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub character: String,
    pub action: String,
    pub message: String,
    #[serde(default)]
    pub board_text: Option<String>,
    pub not_yet_message: String,
    pub schedule: Schedule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Schedule {
    #[serde(rename = "interval")]
    Interval { minutes: u32 },
    #[serde(rename = "once")]
    Once { date: String, time: String },
    #[serde(rename = "daily")]
    Daily { time: String },
    #[serde(rename = "weekly")]
    Weekly { days: Vec<u8>, time: String },
}

/// Legacy config shape from v1 (single interval).
#[derive(Debug, Deserialize)]
struct LegacyConfig {
    reminder_interval_minutes: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 2,
            // Empty by default — never auto-recreate a drink reminder after the user deletes all.
            reminders: vec![],
            character_size: default_character_size(),
            pause: None,
        }
    }
}

pub fn default_water_reminder(minutes: u32) -> Reminder {
    Reminder {
        id: Uuid::new_v4().to_string(),
        name: "Drink water".into(),
        enabled: true,
        character: "water-guy".into(),
        action: "drink".into(),
        message: "Did you remember to drink water?".into(),
        board_text: None,
        not_yet_message: "Drink Now! Get up and drink a glass of water.".into(),
        schedule: Schedule::Interval {
            minutes: minutes.clamp(1, 240),
        },
    }
}

pub fn config_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config.json")
}

pub fn load(app: &tauri::AppHandle) -> Config {
    let path = config_path(app);
    let Some(raw) = std::fs::read_to_string(&path).ok() else {
        return Config::default();
    };

    // Accept empty reminder lists — deleting all reminders must stick.
    if let Ok(cfg) = serde_json::from_str::<Config>(&raw) {
        return cfg;
    }

    // Migrate legacy single-interval config.
    if let Ok(legacy) = serde_json::from_str::<LegacyConfig>(&raw) {
        let cfg = Config {
            version: 2,
            reminders: vec![default_water_reminder(legacy.reminder_interval_minutes)],
            character_size: default_character_size(),
            pause: None,
        };
        save(app, &cfg);
        return cfg;
    }

    Config::default()
}

pub fn save(app: &tauri::AppHandle, cfg: &Config) {
    let path = config_path(app);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}

pub fn validate_reminder(r: &Reminder) -> Result<(), String> {
    if r.name.trim().is_empty() {
        return Err("Name is required".into());
    }
    if r.name.len() > MAX_NAME_LEN {
        return Err(format!("Name must be <= {MAX_NAME_LEN} characters"));
    }
    if r.character.trim().is_empty() {
        return Err("Character is required".into());
    }
    if r.action.trim().is_empty() {
        return Err("Action is required".into());
    }
    if r.message.len() > MAX_MESSAGE_LEN {
        return Err(format!("Message must be <= {MAX_MESSAGE_LEN} characters"));
    }
    if r.message.trim().is_empty() {
        return Err("Message is required".into());
    }
    if r.not_yet_message.len() > MAX_MESSAGE_LEN {
        return Err(format!(
            "Not-yet message must be <= {MAX_MESSAGE_LEN} characters"
        ));
    }
    if let Some(ref bt) = r.board_text {
        let trimmed = bt.trim();
        if is_board_link(trimmed) {
            if trimmed.len() > MAX_BOARD_LINK_LEN {
                return Err(format!(
                    "Board link must be <= {MAX_BOARD_LINK_LEN} characters"
                ));
            }
        } else if trimmed.len() > MAX_BOARD_TEXT_LEN {
            return Err(format!(
                "Board text must be <= {MAX_BOARD_TEXT_LEN} characters"
            ));
        }
    }
    if r.action == "board" {
        let bt = r.board_text.as_deref().unwrap_or("").trim();
        if bt.is_empty() {
            return Err("Board text is required for the board action".into());
        }
        if is_board_link(bt) {
            if bt.len() > MAX_BOARD_LINK_LEN {
                return Err(format!(
                    "Board link must be <= {MAX_BOARD_LINK_LEN} characters"
                ));
            }
        } else if bt.len() < MIN_BOARD_TEXT_LEN || bt.len() > MAX_BOARD_TEXT_LEN {
            return Err(format!(
                "Board text must be {MIN_BOARD_TEXT_LEN}–{MAX_BOARD_TEXT_LEN} characters, or a link"
            ));
        }
    }
    match &r.schedule {
        Schedule::Interval { minutes } => {
            if !(1..=240).contains(minutes) {
                return Err("Interval must be between 1 and 240 minutes".into());
            }
        }
        Schedule::Once { date, time } => {
            parse_date(date)?;
            parse_time(time)?;
        }
        Schedule::Daily { time } => {
            parse_time(time)?;
        }
        Schedule::Weekly { days, time } => {
            parse_time(time)?;
            if days.is_empty() {
                return Err("Pick at least one weekday".into());
            }
            if days.iter().any(|d| *d > 6) {
                return Err("Weekdays must be 0 (Mon) through 6 (Sun)".into());
            }
        }
    }
    Ok(())
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| "Date must be YYYY-MM-DD".into())
}

fn parse_time(s: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(s, "%H:%M").map_err(|_| "Time must be HH:MM".into())
}

fn weekday_to_u8(w: Weekday) -> u8 {
    match w {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    }
}

/// Next fire time for a schedule. For Interval, `from` is the base (usually now).
pub fn next_fire(schedule: &Schedule, from: chrono::DateTime<Local>) -> Option<chrono::DateTime<Local>> {
    match schedule {
        Schedule::Interval { minutes } => {
            Some(from + ChronoDuration::minutes(*minutes as i64))
        }
        Schedule::Once { date, time } => {
            let d = parse_date(date).ok()?;
            let t = parse_time(time).ok()?;
            let dt = d.and_time(t).and_local_timezone(Local).single()?;
            if dt > from {
                Some(dt)
            } else {
                None
            }
        }
        Schedule::Daily { time } => {
            let t = parse_time(time).ok()?;
            let today = from.date_naive().and_time(t);
            let today_local = today.and_local_timezone(Local).single()?;
            if today_local > from {
                Some(today_local)
            } else {
                let tomorrow = (from.date_naive() + ChronoDuration::days(1))
                    .and_time(t)
                    .and_local_timezone(Local)
                    .single()?;
                Some(tomorrow)
            }
        }
        Schedule::Weekly { days, time } => {
            let t = parse_time(time).ok()?;
            let mut best: Option<chrono::DateTime<Local>> = None;
            for offset in 0..14 {
                let day = from.date_naive() + ChronoDuration::days(offset);
                let wd = weekday_to_u8(day.weekday());
                if !days.contains(&wd) {
                    continue;
                }
                let candidate = day.and_time(t).and_local_timezone(Local).single()?;
                if candidate > from {
                    best = Some(candidate);
                    break;
                }
            }
            best
        }
    }
}

pub fn schedule_summary(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Interval { minutes } => format!("Every {minutes} min"),
        Schedule::Once { date, time } => format!("Once {date} {time}"),
        Schedule::Daily { time } => format!("Daily {time}"),
        Schedule::Weekly { days, time } => {
            const NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            let mut sorted = days.clone();
            sorted.sort_unstable();
            sorted.dedup();
            let labels: Vec<&str> = sorted
                .iter()
                .filter_map(|d| NAMES.get(*d as usize).copied())
                .collect();
            format!("{} {}", labels.join("/"), time)
        }
    }
}

#[allow(dead_code)]
pub fn format_next_fire(dt: Option<chrono::DateTime<Local>>) -> Option<String> {
    dt.map(|d| {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            d.year(),
            d.month(),
            d.day(),
            d.hour(),
            d.minute()
        )
    })
}
