use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tokio::sync::mpsc::UnboundedSender;
use windows::{
    core::{w, GUID, PCWSTR},
    Win32::{
        Foundation::HWND,
        System::{
            LibraryLoader::GetModuleHandleW,
            Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING},
            RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification},
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GWL_EXSTYLE,
            GetWindowLongPtrW, HWND_MESSAGE, MSG, PBT_POWERSETTINGCHANGE, RegisterClassW,
            SetWindowLongPtrW, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW, WM_POWERBROADCAST,
            WM_WTSSESSION_CHANGE, WS_EX_LAYERED, WS_EX_TRANSPARENT,
            DEVICE_NOTIFY_WINDOW_HANDLE, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
        },
    },
};

use super::PlatformEvent;

// GUID_CONSOLE_DISPLAY_STATE = {6FE69556-704A-47A0-8F24-C28D936FDA47}
const GUID_DISPLAY_STATE: GUID = GUID {
    data1: 0x6fe69556,
    data2: 0x704a,
    data3: 0x47a0,
    data4: [0x8f, 0x24, 0xc2, 0x8d, 0x93, 0x6f, 0xda, 0x47],
};

/// Returns (width, height, left, top) of the primary monitor work area
/// (excludes taskbar), in physical pixels.
pub fn get_work_area() -> (u32, u32, i32, i32) {
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::SystemParametersInfoW};
    use windows::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    let mut rect = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    let w = (rect.right - rect.left) as u32;
    let h = (rect.bottom - rect.top) as u32;
    (w, h, rect.left, rect.top)
}

/// Toggles WS_EX_TRANSPARENT on the overlay window so clicks pass through
/// during video playback, then disables it when buttons are shown.
pub fn set_clickthrough(window: &tauri::WebviewWindow, enabled: bool) {
    let hwnd = match get_hwnd(window) {
        Some(h) => h,
        None => return,
    };
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_style = if enabled {
            current | (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize
        } else {
            (current | WS_EX_LAYERED.0 as isize) & !(WS_EX_TRANSPARENT.0 as isize)
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
    }
}

fn get_hwnd(window: &tauri::WebviewWindow) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// Spawns a native OS thread with a message-only window that receives
/// WM_WTSSESSION_CHANGE (lock/unlock) and WM_POWERBROADCAST (display off/on),
/// then forwards them as PlatformEvents over the mpsc channel.
pub fn start_monitor(tx: UnboundedSender<PlatformEvent>) {
    std::thread::Builder::new()
        .name("gb-platform-monitor".into())
        .spawn(move || unsafe {
            let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap();
            let class_name = w!("GreedyBee_Monitor_v1");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR::null(),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE, // message-only — no desktop presence
                None,
                hinstance,
                None,
            )
            .unwrap();

            // Register for session lock/unlock events
            let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);

            // Register for monitor display state changes
            let _ = RegisterPowerSettingNotification(
                hwnd.into(),
                &GUID_DISPLAY_STATE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            );

            let mut msg = MSG::default();
            loop {
                let result = GetMessageW(&mut msg, hwnd, 0, 0);
                if result.0 <= 0 {
                    break;
                }

                match msg.message {
                    WM_WTSSESSION_CHANGE => {
                        match msg.wParam.0 as u32 {
                            x if x == WTS_SESSION_LOCK => {
                                let _ = tx.send(PlatformEvent::ScreenOff);
                            }
                            x if x == WTS_SESSION_UNLOCK => {
                                let _ = tx.send(PlatformEvent::ScreenOn);
                            }
                            _ => {}
                        }
                    }
                    WM_POWERBROADCAST => {
                        if msg.wParam.0 as u32 == PBT_POWERSETTINGCHANGE {
                            let setting = &*(msg.lParam.0 as *const POWERBROADCAST_SETTING);
                            match setting.Data[0] {
                                0 => {
                                    let _ = tx.send(PlatformEvent::ScreenOff);
                                }
                                1 | 2 => {
                                    let _ = tx.send(PlatformEvent::ScreenOn);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {
                        DispatchMessageW(&msg);
                    }
                }
            }
        })
        .expect("failed to spawn platform monitor thread");
}
