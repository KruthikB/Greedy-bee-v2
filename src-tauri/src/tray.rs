use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::{scheduler, AppState};

pub fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Settings", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;

    let pause_30 = MenuItem::with_id(app, "pause_30", "Pause for 30 minutes", true, None::<&str>)?;
    let pause_60 = MenuItem::with_id(app, "pause_60", "Pause for 1 hour", true, None::<&str>)?;
    let pause_120 =
        MenuItem::with_id(app, "pause_120", "Pause for 2 hours", true, None::<&str>)?;
    let pause_indef = MenuItem::with_id(
        app,
        "pause_indef",
        "Pause indefinitely",
        true,
        None::<&str>,
    )?;
    let pause_menu = Submenu::with_id_and_items(
        app,
        "pause_menu",
        "Pause for Meeting",
        true,
        &[
            &pause_30,
            &pause_60,
            &pause_120,
            &PredefinedMenuItem::separator(app)?,
            &pause_indef,
        ],
    )?;

    let resume = MenuItem::with_id(app, "resume", "Resume Now", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let test = MenuItem::with_id(app, "test", "Test Reminder Now", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Greedy Bee", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&open, &sep1, &pause_menu, &resume, &sep2, &test, &sep3, &quit],
    )?;

    let Some(icon) = app.default_window_icon() else {
        eprintln!("Greedy Bee: no default window icon; tray was not created");
        return Ok(());
    };

    TrayIconBuilder::with_id("main_tray")
        .icon(icon.clone())
        .tooltip("Greedy Bee")
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                open_settings(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let state = app.state::<AppState>();
    match event.id().as_ref() {
        "open" => open_settings(app),
        "pause_30" => scheduler::pause(&state.scheduler, Some(30), app),
        "pause_60" => scheduler::pause(&state.scheduler, Some(60), app),
        "pause_120" => scheduler::pause(&state.scheduler, Some(120), app),
        "pause_indef" => scheduler::pause(&state.scheduler, None, app),
        "resume" => scheduler::resume(&state.scheduler, app),
        "test" => scheduler::test_reminder(&state.scheduler, None, app),
        "quit" => crate::quit_fully(app),
        _ => {}
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
