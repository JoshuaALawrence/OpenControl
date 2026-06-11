use std::ffi::c_void;
use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindow,
    GetWindowDisplayAffinity, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, MoveWindow,
    PostMessageW, SetForegroundWindow, SetWindowPos, ShowWindow, WindowFromPoint, GA_ROOT,
    GWL_EXSTYLE, GWL_STYLE, GW_HWNDPREV, GW_OWNER, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, WM_CLOSE, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::protocol::Window;

pub fn hwnd_from_id(id: i64) -> HWND {
    HWND(id as *mut c_void)
}
pub fn id_from_hwnd(hwnd: HWND) -> i64 {
    hwnd.0 as i64
}

pub fn is_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd).as_bool() }
}

pub fn foreground_window() -> Option<HWND> {
    let h = unsafe { GetForegroundWindow() };
    if h.0.is_null() {
        None
    } else {
        Some(h)
    }
}

extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let acc = &mut *(lparam.0 as *mut Vec<HWND>);
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return TRUE;
        }
        // Skip tool windows that aren't user task targets.
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return TRUE;
        }
        acc.push(hwnd);
        TRUE
    }
}

pub fn enum_top_level() -> Vec<HWND> {
    let mut acc: Vec<HWND> = Vec::new();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::EnumWindows(
            Some(enum_proc),
            LPARAM(&mut acc as *mut _ as isize),
        );
    }
    acc
}

pub fn window_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn window_pid(hwnd: HWND) -> u32 {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

pub fn window_class(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut buf);
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

/// True if the window is cloaked by DWM (hidden on another virtual desktop, a
/// suspended UWP app, minimized-to-tray, etc.). Cloaked windows are still
/// enumerated but are not actually visible, so they must not count as occluders
/// nor as on-screen blocked windows during redaction.
pub fn is_cloaked(hwnd: HWND) -> bool {
    unsafe {
        let mut cloaked: u32 = 0;
        let res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        );
        res.is_ok() && cloaked != 0
    }
}

/// Gather the facts needed to evaluate the user blocklist against a window.
pub fn window_info(hwnd: HWND) -> crate::blocklist::WindowInfo {
    let pid = window_pid(hwnd);
    crate::blocklist::WindowInfo {
        pid,
        exe_path: process_path(pid),
        title: window_title(hwnd),
        class_name: window_class(hwnd),
        rect: window_rect(hwnd),
    }
}

/// The top-level (root) window under a physical screen point, if any.
pub fn window_at_point(x: i32, y: i32) -> Option<HWND> {
    unsafe {
        let h = WindowFromPoint(POINT { x, y });
        if h.0.is_null() {
            return None;
        }
        let root = GetAncestor(h, GA_ROOT);
        Some(if root.0.is_null() { h } else { root })
    }
}

/// Find visible transient companion windows (menus, dropdowns, popups, tooltips)
/// associated with the target, ordered bottom-to-top by z-order so callers can
/// assign increasing zIndex. These often hold the UI an agent must act on
/// (open menus, combo lists) but are separate top-level windows.
pub fn transient_popups(target: HWND) -> Vec<HWND> {
    let target_pid = window_pid(target);
    // Walk z-order from the target upward (GW_HWNDPREV = window above).
    let mut above: Vec<HWND> = Vec::new();
    unsafe {
        let mut cur = target;
        loop {
            let prev = GetWindow(cur, GW_HWNDPREV);
            match prev {
                Ok(p) if !p.0.is_null() => {
                    if !IsWindow(p).as_bool() {
                        break;
                    }
                    above.push(p);
                    cur = p;
                    if above.len() > 200 {
                        break; // safety bound
                    }
                }
                _ => break,
            }
        }
    }
    // above[0] is just above target, last is topmost. Keep that order so the
    // caller assigns zIndex 1,2,3 with topmost largest.
    let mut out: Vec<HWND> = Vec::new();
    for hwnd in above {
        if !is_transient_companion(hwnd, target, target_pid) {
            continue;
        }
        out.push(hwnd);
        if out.len() >= 6 {
            break; // cap layered captures
        }
    }
    out
}

fn is_transient_companion(hwnd: HWND, target: HWND, target_pid: u32) -> bool {
    if hwnd == target {
        return false;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
    }
    // Must have a non-empty rectangle.
    let rect = match window_rect(hwnd) {
        Some(r) => r,
        None => return false,
    };
    let (l, t, r, b) = rect;
    if (r - l) < 2 || (b - t) < 2 {
        return false;
    }
    let class = window_class(hwnd);
    // Classic transient classes: menus (#32768), tooltips, combo dropdowns.
    let transient_class = matches!(
        class.as_str(),
        "#32768" | "tooltips_class32" | "ComboLBox" | "DropDown" | "Auto-Suggest Dropdown"
    );
    if transient_class {
        return true;
    }
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        // Child windows are captured with their parent, not separately.
        if style & WS_CHILD.0 != 0 {
            return false;
        }
        let is_popup = style & WS_POPUP.0 != 0;
        // Owned by the target window?
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        let owned_by_target = owner == target;
        // Same process popup (e.g. a Qt/Electron popup that owns its own HWND).
        let same_pid_popup = is_popup && window_pid(hwnd) == target_pid;
        owned_by_target || same_pid_popup
    }
}

pub fn process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 260];
        let mut size = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        res.ok()?;
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

pub fn window_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some((r.left, r.top, r.right, r.bottom))
    }
}

pub fn visible_frame_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut r = RECT::default();
        let res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        );
        if res.is_err() || r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some((r.left, r.top, r.right, r.bottom))
    }
}

pub fn capture_origin_for_size(hwnd: HWND, width: i32, height: i32) -> (i32, i32) {
    let visible = visible_frame_rect(hwnd);
    if let Some((l, t, r, b)) = visible {
        if (r - l - width).abs() <= 2 && (b - t - height).abs() <= 2 {
            return (l, t);
        }
    }
    window_rect(hwnd)
        .map(|(l, t, _, _)| (l, t))
        .unwrap_or((0, 0))
}

pub fn display_affinity(hwnd: HWND) -> Option<u32> {
    unsafe {
        let mut affinity = 0u32;
        GetWindowDisplayAffinity(hwnd, &mut affinity).ok()?;
        Some(affinity)
    }
}

pub fn display_affinity_name(value: u32) -> &'static str {
    match value {
        0x0000_0000 => "none",
        0x0000_0001 => "monitor",
        0x0000_0011 => "exclude_from_capture",
        _ => "unknown",
    }
}

pub fn to_window(hwnd: HWND) -> Window {
    let app = process_path(window_pid(hwnd)).unwrap_or_else(|| "unknown".to_string());
    Window {
        app,
        id: id_from_hwnd(hwnd),
        title: window_title(hwnd),
    }
}

pub fn list_windows() -> Vec<Window> {
    enum_top_level().into_iter().map(to_window).collect()
}

pub fn activate(hwnd: HWND) -> bool {
    if !is_window(hwnd) {
        return false;
    }
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = BringWindowToTop(hwnd);
        if SetForegroundWindow(hwnd).as_bool() {
            return true;
        }
        // Foreground-lock workaround: attach to the current foreground thread.
        let fg = GetForegroundWindow();
        let target_thread = GetWindowThreadProcessId(hwnd, None);
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let _ = AttachThreadInput(fg_thread, target_thread, true);
        let _ = BringWindowToTop(hwnd);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(fg_thread, target_thread, false);
        ok
    }
}

pub fn launch_app(app: &str) -> Result<(), String> {
    use std::process::Command;
    let app = app.trim();
    if app.is_empty() {
        return Err("empty app identifier".into());
    }
    // Direct spawn for a concrete executable / path.
    let looks_like_path =
        app.contains('\\') || app.contains('/') || app.to_ascii_lowercase().ends_with(".exe");
    if looks_like_path {
        match Command::new(app).spawn() {
            Ok(_) => return Ok(()),
            Err(e) => return Err(format!("spawn failed: {e}")),
        }
    }
    // Otherwise use the shell so aliases / URIs (calc, notepad, ms-settings:) resolve.
    match Command::new("cmd").args(["/c", "start", "", app]).spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("shell start failed: {e}")),
    }
}

/// Set window state: "minimize" | "maximize" | "restore".
pub fn window_state(hwnd: HWND, state: &str) -> Result<(), String> {
    let cmd = match state.trim().to_ascii_lowercase().as_str() {
        "minimize" | "min" => SW_MINIMIZE,
        "maximize" | "max" => SW_MAXIMIZE,
        "restore" | "normal" => SW_RESTORE,
        other => return Err(format!("unknown window state: {other}")),
    };
    unsafe {
        let _ = ShowWindow(hwnd, cmd);
    }
    Ok(())
}

/// Request a window to close (posts WM_CLOSE; the app may prompt to save).
pub fn close_window(hwnd: HWND) -> Result<(), String> {
    unsafe {
        PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)).map_err(|e| format!("close failed: {e}"))
    }
}

/// Pin a window above (or unpin from) all others.
pub fn set_topmost(hwnd: HWND, enabled: bool) -> Result<(), String> {
    let target = if enabled {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    unsafe {
        SetWindowPos(
            hwnd,
            target,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .map_err(|e| format!("set_topmost failed: {e}"))
    }
}

/// Move/resize a window to absolute virtual-desktop pixels (restores if min/maxed).
pub fn set_window_bounds(
    hwnd: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    unsafe {
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        MoveWindow(hwnd, x, y, width, height, true).map_err(|e| format!("move failed: {e}"))
    }
}

#[allow(dead_code)]
pub fn unused_void(_p: *mut c_void) {}
