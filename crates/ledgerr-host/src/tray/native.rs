//! Windows-native system tray implementation using `Shell_NotifyIconW`.
//!
//! Replaces the `tray-icon` crate with direct Win32 API calls via `windows-rs`,
//! enabling animated icons and taskbar progress overlays in the future.
//!
//! # Architecture
//!
//! A hidden message-only window is created to receive `NOTIFYICONDATAW` callback
//! messages (via `WM_APP + 1`) and context-menu `WM_COMMAND` events. A dedicated
//! thread runs the message pump and forwards menu events to the main thread
//! through an `mpsc` channel.

#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc;
use std::time::Duration;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ── Menu Command IDs ──────────────────────────────────────────────────────────
// Each tray menu item is assigned a stable command ID dispatched via WM_COMMAND.
// Info/status items (greyed out) share the same ID namespace.

pub(crate) const CMD_VERSION: u32 = 100;
pub(crate) const CMD_BACKEND: u32 = 101;
pub(crate) const CMD_TOAST_ENABLED: u32 = 102;
pub(crate) const CMD_CYCLE_BACKEND: u32 = 103;
pub(crate) const CMD_LAST_TEST: u32 = 104;
pub(crate) const CMD_START_MINIMIZED: u32 = 105;
pub(crate) const CMD_WINDOW_VISIBLE: u32 = 106;
pub(crate) const CMD_NOTIFY_APPROVAL: u32 = 107;
pub(crate) const CMD_NOTIFY_SUBMITTED: u32 = 108;
pub(crate) const CMD_NOTIFY_FAILED: u32 = 109;
pub(crate) const CMD_NOTIFY_COMPLETED: u32 = 110;
pub(crate) const CMD_TEST_TOAST: u32 = 111;
pub(crate) const CMD_STATUS: u32 = 112;
pub(crate) const CMD_SHOW_WINDOW: u32 = 113;
pub(crate) const CMD_EXIT: u32 = 114;

/// Items rendered with a checkmark that toggle on click.
pub(crate) const CHECK_ITEM_IDS: &[u32] = &[
    CMD_TOAST_ENABLED,
    CMD_START_MINIMIZED,
    CMD_WINDOW_VISIBLE,
    CMD_NOTIFY_APPROVAL,
    CMD_NOTIFY_SUBMITTED,
    CMD_NOTIFY_FAILED,
    CMD_NOTIFY_COMPLETED,
];

/// Items whose text is updated dynamically at runtime (always greyed info rows).
pub(crate) const DYNAMIC_TEXT_IDS: &[u32] = &[
    CMD_VERSION,
    CMD_BACKEND,
    CMD_LAST_TEST,
    CMD_STATUS,
];

// ── Channel Types ─────────────────────────────────────────────────────────────

/// Events sent from the tray message-pump thread to the main event loop.
#[derive(Debug, Clone)]
pub(crate) enum TrayEvent {
    /// A menu item was selected; payload is the command ID.
    MenuCommand(u32),
}

/// Commands sent from the main thread to the tray thread to update menu state.
#[derive(Debug)]
pub(crate) enum TrayControl {
    /// Refresh all dynamic labels and check states from the current application state.
    UpdateLabels {
        version: String,
        backend: String,
        last_test: String,
        status: String,
        toast_enabled: bool,
        start_minimized: bool,
        window_visible: bool,
        notify_approval: bool,
        notify_submitted: bool,
        notify_failed: bool,
        notify_completed: bool,
    },
    /// Gracefully shut down the tray thread.
    Quit,
}

// ── Per-Window User Data ─────────────────────────────────────────────────────
// Stored via GWLP_USERDATA so the static window procedure can reach it.

struct WindowUserData {
    hmenu: HMENU,
    event_tx: mpsc::Sender<TrayEvent>,
}

// ── Icon Data Generation ──────────────────────────────────────────────────────
// Moved from runtime.rs: produces the RGBA pixel buffer for the 16×16 tray icon.

pub(crate) fn make_icon_data() -> (Vec<u8>, i32, i32) {
    let width = 16i32;
    let height = 16i32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
            let fill = (2..=13).contains(&x) && (2..=13).contains(&y);
            let stem = (4..=6).contains(&x) && (4..=11).contains(&y);
            let foot = (4..=11).contains(&x) && (10..=12).contains(&y);

            let pixel = if border {
                [0x0D, 0x47, 0xA1, 0xFF]
            } else if stem || foot {
                [0xFF, 0xFF, 0xFF, 0xFF]
            } else if fill {
                [0x19, 0x7A, 0xD9, 0xFF]
            } else {
                [0x00, 0x00, 0x00, 0x00]
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    (rgba, width, height)
}

// ── Icon Creation (RGBA → HICON) ──────────────────────────────────────────────

/// Create a Windows `HICON` from raw RGBA pixel data.
///
/// Uses `CreateDIBSection` for the 32-bit colour bitmap and `CreateIconIndirect`
/// to assemble the final icon handle. The monochrome mask is zero-initialised so
/// the alpha channel in the colour bitmap drives transparency.
unsafe fn create_icon_from_rgba(
    rgba: &[u8],
    width: i32,
    height: i32,
) -> Result<HICON, Box<dyn std::error::Error>> {
    let hdc = GetDC(None);
    if hdc.is_invalid() {
        return Err("GetDC failed — no display device context".into());
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        bmiColors: [RGBQUAD::default(); 1],
    };

    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let color_bmp = CreateDIBSection(
        Some(hdc),
        &bmi as *const BITMAPINFO,
        DIB_RGB_COLORS,
        &mut bits as *mut *mut core::ffi::c_void,
        None,
        0,
    )?;

    if bits.is_null() {
        let _ = DeleteObject(color_bmp);
        let _ = ReleaseDC(None, hdc);
        return Err("CreateDIBSection returned null pixel pointer".into());
    }

    // Copy the RGBA data into the DIB backing store.
    core::ptr::copy_nonoverlapping(rgba.as_ptr(), bits as *mut u8, rgba.len());

    // Create a zero-initialised monochrome mask (all zeros = alpha drives opacity).
    let mask_row_bytes = ((width + 15) / 16) * 2; // word-aligned scanline
    let mask_size = (mask_row_bytes * height) as usize;
    let mask_data = vec![0u8; mask_size];
    let mask_bmp = CreateBitmap(width, height, 1, 1, Some(mask_data.as_ptr() as *const _));
    if mask_bmp.is_invalid() {
        let _ = DeleteObject(color_bmp);
        let _ = ReleaseDC(None, hdc);
        return Err("CreateBitmap for monochrome mask failed".into());
    }

    let icon_info = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bmp,
        hbmColor: color_bmp,
    };

    let hicon = CreateIconIndirect(&icon_info as *const ICONINFO)?;

    // The icon handle owns its own copy; the originals can be freed.
    let _ = DeleteObject(mask_bmp);
    let _ = DeleteObject(color_bmp);
    let _ = ReleaseDC(None, hdc);

    Ok(hicon)
}

// ── Window Procedure ──────────────────────────────────────────────────────────

/// Static window procedure for the hidden tray message window.
///
/// - [`WM_APP + 1`]: Shell_NotifyIcon callback — shows the context menu on
///   right-click.
/// - [`WM_COMMAND`]: Menu item selection — forwards the command ID to the main
///   thread via [`WindowUserData.event_tx`].
/// - [`WM_DESTROY`]: Posts [`WM_QUIT`] to terminate the message pump.
unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP + 1 => {
            // Shell_NotifyIcon callback — LOWORD(lparam) holds the mouse message.
            match lparam.0 as u32 {
                WM_RBUTTONUP => {
                    // Right-click: show the context menu at cursor position.
                    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                    if ptr != 0 {
                        let user_data = &*(ptr as *const WindowUserData);
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let _ = SetForegroundWindow(hwnd);
                        TrackPopupMenu(
                            user_data.hmenu,
                            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                            pt.x,
                            pt.y,
                            None,
                            hwnd,
                            None,
                        );
                        // Required to properly dismiss the menu on selection.
                        let _ = PostMessageW(hwnd, WM_NULL, WPARAM::default(), LPARAM::default());
                    }
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_COMMAND => {
            // HIGHWORD(wparam) = 0 for menu selection, 1 for accelerator.
            let source_hiword = ((wparam.0 as u32) >> 16) & 0xFFFF;
            if source_hiword == 0 {
                let id = (wparam.0 as u32) & 0xFFFF;
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr != 0 {
                    let user_data = &*(ptr as *const WindowUserData);
                    let _ = user_data.event_tx.send(TrayEvent::MenuCommand(id));
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── Menu Construction ─────────────────────────────────────────────────────────

/// Build the popup menu with all tray items.
///
/// Menu layout mirrors the original `tray-icon` implementation:
/// 1. Info items (greyed): version, backend, last_test, status
/// 2. Action items: toast_enabled, cycle_backend, start_minimized, etc.
/// 3. Check items: notification toggles
/// 4. Show window, Exit
unsafe fn build_tray_menu(
    labels: &crate::tray::TrayMenuLabels,
) -> Result<HMENU, Box<dyn std::error::Error>> {
    let hmenu = CreatePopupMenu()?;

    // ── helpers ──────────────────────────────────────────────────────────
    fn push_info(hmenu: HMENU, id: u32, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(core::iter::once(0)).collect();
        AppendMenuW(hmenu, MF_STRING | MF_GRAYED, id as usize, PCWSTR::from_raw(wide.as_ptr()))?;
        Ok(())
    }

    fn push_action(hmenu: HMENU, id: u32, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(core::iter::once(0)).collect();
        AppendMenuW(hmenu, MF_STRING, id as usize, PCWSTR::from_raw(wide.as_ptr()))?;
        Ok(())
    }

    fn push_check(hmenu: HMENU, id: u32, text: &str, checked: bool) -> Result<(), Box<dyn std::error::Error>> {
        let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(core::iter::once(0)).collect();
        let flags = if checked {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING | MF_UNCHECKED
        };
        AppendMenuW(hmenu, flags, id as usize, PCWSTR::from_raw(wide.as_ptr()))?;
        Ok(())
    }

    // ── build ────────────────────────────────────────────────────────────
    push_info(hmenu, CMD_VERSION, &labels.version)?;
    push_info(hmenu, CMD_BACKEND, &labels.backend)?;
    push_action(hmenu, CMD_TOAST_ENABLED, labels.toast_enabled)?;
    push_action(hmenu, CMD_CYCLE_BACKEND, labels.cycle_backend)?;
    push_info(hmenu, CMD_LAST_TEST, &labels.last_test)?;
    push_action(hmenu, CMD_START_MINIMIZED, labels.start_minimized_to_tray)?;
    push_action(hmenu, CMD_WINDOW_VISIBLE, labels.window_visible_on_start)?;
    push_action(hmenu, CMD_NOTIFY_APPROVAL, labels.notify_approval_required)?;
    push_action(hmenu, CMD_NOTIFY_SUBMITTED, labels.notify_transaction_submitted)?;
    push_action(hmenu, CMD_NOTIFY_FAILED, labels.notify_run_failed)?;
    push_action(hmenu, CMD_NOTIFY_COMPLETED, labels.notify_run_completed)?;
    push_action(hmenu, CMD_TEST_TOAST, labels.test_toast)?;
    push_info(hmenu, CMD_STATUS, &labels.status)?;
    push_action(hmenu, CMD_SHOW_WINDOW, labels.show_window)?;
    push_action(hmenu, CMD_EXIT, labels.exit)?;

    Ok(hmenu)
}

// ── Menu Update Helpers ───────────────────────────────────────────────────────

/// Update the text of a greyed info item.
unsafe fn update_info_text(hmenu: HMENU, id: u32, text: &str) {
    let wide: Vec<u16> = OsStr::new(text)
        .encode_wide()
        .chain(core::iter::once(0))
        .collect();
    let _ = ModifyMenuW(hmenu, id, MF_BYCOMMAND | MF_STRING | MF_GRAYED, id as usize, PCWSTR::from_raw(wide.as_ptr()));
}

/// Toggle the checkmark state of a menu item.
unsafe fn set_menu_check(hmenu: HMENU, id: u32, checked: bool) {
    let flag = if checked { MF_CHECKED.0 } else { MF_UNCHECKED.0 };
    CheckMenuItem(hmenu, id, flag);
}

// ── Native Tray Platform ──────────────────────────────────────────────────────

/// Owns the Windows tray icon lifecycle: hidden window, `Shell_NotifyIconW`,
/// context menu, and message-pump thread.
///
/// Dropping this struct sends [`TrayControl::Quit`] and joins the thread.
pub(crate) struct NativeTrayPlatform {
    /// Sender for control commands to the tray thread.
    pub(crate) control_tx: mpsc::Sender<TrayControl>,
    /// Receiver for menu events from the tray thread.
    pub(crate) event_rx: mpsc::Receiver<TrayEvent>,
    /// Join handle for the message-pump thread.
    pub(crate) thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl NativeTrayPlatform {
    /// Spawn a background thread that owns the native tray window and message pump.
    ///
    /// Blocks until the hidden window and tray icon are registered, then returns
    /// the control handles. The thread runs until [`TrayControl::Quit`] is sent.
    pub(crate) fn spawn(
        tooltip: &str,
        icon_rgba: Vec<u8>,
        icon_width: i32,
        icon_height: i32,
        initial_labels: &crate::tray::TrayMenuLabels,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (control_tx, control_rx) = mpsc::channel::<TrayControl>();
        let (event_tx, event_rx) = mpsc::channel::<TrayEvent>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let tooltip_owned = tooltip.to_owned();
        let labels_clone = initial_labels.clone();

        let thread_handle = std::thread::Builder::new()
            .name("tray-msg-pump".into())
            .spawn(move || {
                if let Err(e) = unsafe {
                    run_tray_pump(
                        &tooltip_owned,
                        &icon_rgba,
                        icon_width,
                        icon_height,
                        &labels_clone,
                        event_tx,
                        control_rx,
                        ready_tx,
                    )
                } {
                    // Only report setup errors — pump errors are internal.
                    if !ready_tx.is_closed() {
                        let _ = ready_tx.send(Err(e.to_string()));
                    }
                }
            })?;

        // Block until the pump thread has created the window and registered the icon.
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                control_tx,
                event_rx,
                thread_handle: Some(thread_handle),
            }),
            Ok(Err(msg)) => Err(msg.into()),
            Err(e) => Err(e.into()),
        }
    }

    /// Signal the pump thread to quit and wait for it to finish.
    pub(crate) fn shutdown(&mut self) {
        let _ = self.control_tx.send(TrayControl::Quit);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

// ── Tray Message Pump ─────────────────────────────────────────────────────────

/// Entry point for the pump thread: creates the window, tray icon, and menu, then
/// enters a `PeekMessageW` / control-channel hybrid loop.
///
/// Returns only when [`TrayControl::Quit`] is received or the channel disconnects,
/// having cleaned up all OS resources.
unsafe fn run_tray_pump(
    tooltip: &str,
    icon_rgba: &[u8],
    icon_width: i32,
    icon_height: i32,
    labels: &crate::tray::TrayMenuLabels,
    event_tx: mpsc::Sender<TrayEvent>,
    control_rx: mpsc::Receiver<TrayControl>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let hinstance = GetModuleHandleW(None)?;

    // ── Register window class ────────────────────────────────────────────
    let class_name = windows::core::w!("L3dg3rrTrayWindow");
    let wc = WNDCLASSW {
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(tray_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: HICON::default(),
        hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
    };
    let _atom = RegisterClassW(&wc as *const WNDCLASSW);
    // Failure is benign if the class was already registered.

    // ── Build menu ───────────────────────────────────────────────────────
    let hmenu = build_tray_menu(labels)?;

    // ── User data (stored in window via GWLP_USERDATA) ───────────────────
    let user_data = Box::new(WindowUserData {
        hmenu,
        event_tx: event_tx.clone(),
    });
    let user_data_ptr = Box::into_raw(user_data);

    // ── Create hidden message window ─────────────────────────────────────
    let hwnd = match CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        w!(""),
        WINDOW_STYLE(WS_POPUP.0),
        0,
        0,
        0,
        0,
        None,
        None,
        Some(hinstance),
        Some(user_data_ptr as *const _),
    ) {
        Ok(h) => h,
        Err(e) => {
            let _ = Box::from_raw(user_data_ptr);
            let _ = DestroyMenu(hmenu);
            return Err(format!("CreateWindowExW failed: {e}").into());
        }
    };

    // Store user-data pointer so the window proc can reach it.
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, user_data_ptr as isize);

    // ── Create icon ──────────────────────────────────────────────────────
    let hicon = create_icon_from_rgba(icon_rgba, icon_width, icon_height)?;

    // ── Build tooltip ────────────────────────────────────────────────────
    let tooltip_utf16: Vec<u16> = OsStr::new(tooltip)
        .encode_wide()
        .chain(core::iter::once(0u16))
        .collect();
    let tip_len = tooltip_utf16.len().min(127);

    // ── NOTIFYICONDATAW ──────────────────────────────────────────────────
    let mut nid = NOTIFYICONDATAW {
        cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_APP + 1,
        hIcon: hicon,
        szTip: [0u16; 128],
        ..Default::default()
    };
    nid.szTip[..tip_len].copy_from_slice(&tooltip_utf16[..tip_len]);

    // ── Register tray icon ───────────────────────────────────────────────
    if !Shell_NotifyIconW(NIM_ADD, &nid as *const NOTIFYICONDATAW).as_bool() {
        let _ = DestroyIcon(hicon);
        let _ = DestroyWindow(hwnd);
        let _ = Box::from_raw(user_data_ptr);
        let _ = DestroyMenu(hmenu);
        return Err("Shell_NotifyIconW NIM_ADD failed".into());
    }

    // Signal the main thread that setup is complete.
    let _ = ready_tx.send(Ok(()));

    // ── Message pump + control loop ──────────────────────────────────────
    let mut msg = MSG::default();
    let mut nid_current = nid;
    let mut hicon_current = hicon;

    // Helper: tear down OS resources.
    let mut cleanup = |nid: &NOTIFYICONDATAW, hicon: HICON, hwnd: HWND, hmenu: HMENU, ptr: *mut WindowUserData| {
        let _ = Shell_NotifyIconW(NIM_DELETE, nid as *const NOTIFYICONDATAW);
        let _ = DestroyIcon(hicon);
        let _ = DestroyWindow(hwnd);
        let _ = DestroyMenu(hmenu);
        let _ = Box::from_raw(ptr);
    };

    loop {
        // Pump all pending Win32 messages.
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                // WM_QUIT means DestroyWindow was called — still clean up.
                break;
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        if msg.message == WM_QUIT {
            cleanup(&nid_current, hicon_current, hwnd, hmenu, user_data_ptr);
            return Ok(());
        }

        // Check for control-channel commands.
        match control_rx.try_recv() {
            Ok(TrayControl::UpdateLabels {
                version,
                backend,
                last_test,
                status,
                toast_enabled,
                start_minimized,
                window_visible,
                notify_approval,
                notify_submitted,
                notify_failed,
                notify_completed,
            }) => {
                update_info_text(hmenu, CMD_VERSION, &version);
                update_info_text(hmenu, CMD_BACKEND, &backend);
                update_info_text(hmenu, CMD_LAST_TEST, &last_test);
                update_info_text(hmenu, CMD_STATUS, &status);

                set_menu_check(hmenu, CMD_TOAST_ENABLED, toast_enabled);
                set_menu_check(hmenu, CMD_START_MINIMIZED, start_minimized);
                set_menu_check(hmenu, CMD_WINDOW_VISIBLE, window_visible);
                set_menu_check(hmenu, CMD_NOTIFY_APPROVAL, notify_approval);
                set_menu_check(hmenu, CMD_NOTIFY_SUBMITTED, notify_submitted);
                set_menu_check(hmenu, CMD_NOTIFY_FAILED, notify_failed);
                set_menu_check(hmenu, CMD_NOTIFY_COMPLETED, notify_completed);
            }
            Ok(TrayControl::Quit) => {
                cleanup(&nid_current, hicon_current, hwnd, hmenu, user_data_ptr);
                return Ok(());
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                cleanup(&nid_current, hicon_current, hwnd, hmenu, user_data_ptr);
                return Ok(());
            }
        }

        // Sleep briefly to keep CPU usage low while still responsive.
        std::thread::sleep(Duration::from_millis(10));
    }
}
