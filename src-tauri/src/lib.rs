use std::sync::{Arc, Mutex};
use tauri::{Manager, WindowEvent};

pub mod commands;
pub mod config;
pub mod overlay_window;
pub mod platform;
pub mod scheduler;
pub mod tray;

pub struct AppState {
    pub scheduler: Arc<Mutex<scheduler::SchedulerState>>,
    /// True once the overlay page has registered its reminder-fire listener.
    pub overlay_ready: Arc<Mutex<bool>>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let cfg = config::load(app.handle());
            let scheduler_state = Arc::new(Mutex::new(scheduler::SchedulerState::from_config(&cfg)));

            app.manage(AppState {
                scheduler: scheduler_state.clone(),
                overlay_ready: Arc::new(Mutex::new(false)),
            });

            // Overlay is created on demand when a reminder fires (see overlay_window).

            if let Err(err) = tray::build_tray(app) {
                eprintln!("Greedy Bee: tray icon was not created: {err}");
            }

            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };
                let shortcut =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyW);
                let state_ref = scheduler_state.clone();
                let handle_ref = app.handle().clone();
                if let Err(err) = app.global_shortcut().on_shortcut(
                    shortcut,
                    move |_app, _sc, event| {
                        if event.state() == ShortcutState::Pressed {
                            scheduler::toggle_pause_one_hour(&state_ref, &handle_ref);
                        }
                    },
                ) {
                    eprintln!("Greedy Bee: global shortcut was not registered: {err}");
                }
            }

            #[cfg(target_os = "windows")]
            {
                let (tx, mut rx) =
                    tokio::sync::mpsc::unbounded_channel::<platform::PlatformEvent>();
                platform::start_monitor(tx);
                let state_mon = scheduler_state.clone();
                let handle_mon = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(evt) = rx.recv().await {
                        match evt {
                            platform::PlatformEvent::ScreenOff => {
                                scheduler::pause(&state_mon, None, &handle_mon);
                            }
                            platform::PlatformEvent::ScreenOn => {
                                scheduler::resume(&state_mon, &handle_mon);
                            }
                        }
                    }
                });
            }

            let state_sched = scheduler_state.clone();
            let handle_sched = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                scheduler::run_loop(state_sched, handle_sched).await;
            });

            if let Some(settings) = app.get_webview_window("settings") {
                let _ = settings.show();
                let _ = settings.unminimize();
                let _ = settings.set_focus();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_reminders,
            commands::save_reminder,
            commands::delete_reminder,
            commands::set_reminder_enabled,
            commands::set_interval,
            commands::pause_reminders,
            commands::resume_reminders,
            commands::test_reminder,
            commands::dismiss_overlay,
            commands::overlay_ready,
            commands::set_overlay_clickthrough,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("error while running Greedy Bee: {err}");
            #[cfg(target_os = "windows")]
            platform::show_error(&format!("Greedy Bee failed to start:\n{err}"));
        });
}
