use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Create the transparent overlay window on demand (not at startup) so a hidden
/// WebView2 compositor does not burn CPU/GPU while idle.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let (mut work_w, work_h, work_x, work_y) = crate::platform::get_work_area();
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
        // Logical CSS height for the overlay chrome. Must match overlay.css
        // (video + button strip). Converting a "physical" constant by /scale
        // made the WebView shorter than the CSS on HiDPI and cropped heads.
        let overlay_logical_h = 580.0;
        let overlay_physical_h = (overlay_logical_h * scale).round() as i32;

        let window = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay/index.html".into()))
            .title("")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .visible(true)
            .drag_and_drop(false)
            .position(
                work_x as f64 / scale,
                (work_y + work_h as i32 - overlay_physical_h) as f64 / scale,
            )
            .inner_size(work_w as f64 / scale, overlay_logical_h)
            .build()?;

        // Click-through until the reminder UI asks for buttons.
        crate::platform::set_clickthrough(&window, true);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let window = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay/index.html".into()))
            .title("")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(true)
            .build()?;
        crate::platform::set_clickthrough(&window, true);
    }

    Ok(())
}

pub fn destroy(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
}
