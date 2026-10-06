use std::sync::{Arc, Mutex};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub mod commands;
pub mod config;
pub mod platform;
pub mod scheduler;
pub mod tray;

pub struct AppState {
    pub scheduler: Arc<Mutex<scheduler::SchedulerState>>,
}

pub fn run() {
    let scheduler_state = Arc::new(Mutex::new(scheduler::SchedulerState::new()));

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
        .setup({
            let scheduler_state = scheduler_state.clone();
            move |app| {
                // Load config and apply to scheduler
                let cfg = config::load(app.handle());
                {
                    let mut s = scheduler_state.lock().unwrap();
                    s.interval_minutes = cfg.reminder_interval_minutes;
                    let now = std::time::Instant::now();
                    s.next_reminder_at =
                        now + std::time::Duration::from_secs(s.interval_minutes as u64 * 60);
                }

                // Register managed state
                app.manage(AppState {
                    scheduler: scheduler_state.clone(),
                });

                // Build overlay window programmatically so we can use live screen geometry
                #[cfg(target_os = "windows")]
                {
                    let (mut work_w, work_h, work_x, work_y) = platform::get_work_area();
                    if work_w == 0 {
                        work_w = 1280;
                    }
                    let mut scale = app
                        .primary_monitor()
                        .ok()
                        .flatten()
                        .map(|m| m.scale_factor())
                        .unwrap_or(1.0);
                    if !scale.is_finite() || scale < 0.5 {
                        scale = 1.0;
                    }
                    let overlay_physical_h = 370u32; // 300 video + 70 button strip

                    if let Err(err) = WebviewWindowBuilder::new(
                        app,
                        "overlay",
                        WebviewUrl::App("overlay/index.html".into()),
                    )
                    .title("")
                    .transparent(true)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .shadow(false)
                    .visible(false)
                    .drag_and_drop(false)
                    .additional_browser_args("--disable-direct-composition-video-overlays")
                    .position(
                        work_x as f64 / scale,
                        (work_y + work_h as i32 - overlay_physical_h as i32) as f64 / scale,
                    )
                    .inner_size(
                        work_w as f64 / scale,
                        overlay_physical_h as f64 / scale,
                    )
                    .build()
                    {
                        eprintln!("Greedy Bee: overlay window was not created: {err}");
                    }
                }

                // Build system tray. A tray failure must not close the settings window.
                if let Err(err) = tray::build_tray(app) {
                    eprintln!("Greedy Bee: tray icon was not created: {err}");
                }

                // Register Ctrl+Shift+W global hotkey → toggle 1-hour pause
                {
                    use tauri_plugin_global_shortcut::{
                        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                    };
                    let shortcut = Shortcut::new(
                        Some(Modifiers::CONTROL | Modifiers::SHIFT),
                        Code::KeyW,
                    );
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

                // Start platform monitor (session lock / power events) — Windows only
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

                // Start the reminder scheduler loop
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
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::set_interval,
            commands::pause_reminders,
            commands::resume_reminders,
            commands::test_reminder,
            commands::dismiss_overlay,
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
