pub mod keysym;
use std::{thread, time::Duration};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

const WHEEL_DELTA: i32 = 120;

fn pause() {
    thread::sleep(Duration::from_millis(8));
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

fn mouse_input(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Move the cursor to an absolute virtual-desktop pixel using normalized coords.
pub fn move_abs(x: i32, y: i32) {
    let (xv, yv, cx, cy) = virtual_screen();
    let cx = (cx - 1).max(1);
    let cy = (cy - 1).max(1);
    let nx = (((x - xv) as i64) * 65535 / cx as i64) as i32;
    let ny = (((y - yv) as i64) * 65535 / cy as i64) as i32;
    send(&[mouse_input(
        nx,
        ny,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )]);
    pause();
}

/// Current cursor position in physical screen pixels.
fn cursor_pos() -> (i32, i32) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

/// Move to a target with cubic ease-out interpolation (~60 Hz), like a human hand.
pub fn move_smooth(x: i32, y: i32) {
    let (sx, sy) = cursor_pos();
    let dx = (x - sx) as f64;
    let dy = (y - sy) as f64;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 8.0 {
        move_abs(x, y);
        return;
    }
    // ~1 frame per 40px, clamped to [6, 18] frames (~50-150ms total).
    let frames = ((dist / 40.0).round() as i32).clamp(6, 18);
    for i in 1..=frames {
        let t = i as f64 / frames as f64;
        let eased = 1.0 - (1.0 - t).powi(3); // cubic ease-out
        let cx = sx as f64 + dx * eased;
        let cy = sy as f64 + dy * eased;
        move_abs(cx.round() as i32, cy.round() as i32);
    }
}

fn button_flags(button: &str) -> (MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS) {
    match button.trim().to_ascii_lowercase().as_str() {
        "right" | "r" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        "middle" | "m" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
    }
}

/// Click at an absolute screen pixel (cursor eases to the target first).
pub fn click(x: i32, y: i32, button: &str, count: u32) {
    crate::interrupt::mark_input();
    move_smooth(x, y);
    let (down, up) = button_flags(button);
    for _ in 0..count.max(1) {
        send(&[mouse_input(0, 0, 0, down)]);
        pause();
        send(&[mouse_input(0, 0, 0, up)]);
        pause();
    }
}

/// Press-drag-release from one absolute screen pixel to another.
pub fn drag(from: (i32, i32), to: (i32, i32), button: &str) {
    crate::interrupt::mark_input();
    let (down, up) = button_flags(button);
    move_abs(from.0, from.1);
    send(&[mouse_input(0, 0, 0, down)]);
    pause();
    // A few interpolated steps make drags land reliably in most apps.
    let steps = 24;
    for i in 1..=steps {
        let x = from.0 + (to.0 - from.0) * i / steps;
        let y = from.1 + (to.1 - from.1) * i / steps;
        move_abs(x, y);
    }
    send(&[mouse_input(0, 0, 0, up)]);
    pause();
}

/// Scroll wheel at an absolute screen pixel. Deltas are in wheel notches.
pub fn scroll(x: i32, y: i32, scroll_x: i32, scroll_y: i32) {
    crate::interrupt::mark_input();
    move_abs(x, y);
    if scroll_y != 0 {
        send(&[mouse_input(0, 0, scroll_y * WHEEL_DELTA, MOUSEEVENTF_WHEEL)]);
        pause();
    }
    if scroll_x != 0 {
        send(&[mouse_input(
            0,
            0,
            scroll_x * WHEEL_DELTA,
            MOUSEEVENTF_HWHEEL,
        )]);
        pause();
    }
}

/// Type arbitrary Unicode text into the focused control.
pub fn type_text(text: &str) {
    type_text_paced(text, 0);
}

/// Type text with an optional per-character delay (ms).
///
/// A small delay is a reliability fallback for apps that drop characters from a
/// fast `SendInput` burst (some Electron / web views debounce keystrokes).
pub fn type_text_paced(text: &str, per_key_delay_ms: u64) {
    crate::interrupt::mark_input();
    for unit in text.encode_utf16() {
        send(&[key_input(0, unit, KEYEVENTF_UNICODE)]);
        send(&[key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)]);
        if per_key_delay_ms > 0 {
            thread::sleep(Duration::from_millis(per_key_delay_ms));
        }
    }
}

fn key_down(vk: u16, extended: bool) {
    let flags = if extended {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    send(&[key_input(vk, 0, flags)]);
}

fn key_up(vk: u16, extended: bool) {
    let mut flags = KEYEVENTF_KEYUP;
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    send(&[key_input(vk, 0, flags)]);
}

/// Press a `+`-separated chord such as `Control_L+Shift_L+period`.
pub fn press_chord(spec: &str) -> Result<(), String> {
    crate::interrupt::mark_input();
    // Split on '+', but treat a trailing empty token (from "ctrl++") as the '+' key.
    let raw: Vec<&str> = spec.split('+').collect();
    let mut tokens: Vec<String> = Vec::new();
    for (i, part) in raw.iter().enumerate() {
        let p = part.trim();
        if p.is_empty() {
            if i > 0 {
                tokens.push("equal".to_string()); // '+' lives on the '=' key
            }
        } else {
            tokens.push(p.to_string());
        }
    }
    if tokens.is_empty() {
        return Err("empty key chord".into());
    }

    let mut held: Vec<(u16, bool)> = Vec::new();
    // Hold every leading modifier, then tap the final (non-modifier) key.
    let last = tokens.len() - 1;
    for (i, tok) in tokens.iter().enumerate() {
        let is_mod = keysym::is_modifier(tok);
        match keysym::resolve(tok) {
            Some((vk, ext)) => {
                if is_mod && i != last {
                    key_down(vk, ext);
                    held.push((vk, ext));
                    pause();
                } else {
                    // main key (or a modifier used as the final key)
                    key_down(vk, ext);
                    pause();
                    key_up(vk, ext);
                    pause();
                }
            }
            None => {
                // Unknown token: type its single character if possible.
                if tok.chars().count() == 1 {
                    type_text(tok);
                } else {
                    // release any held modifiers before erroring
                    for (vk, ext) in held.iter().rev() {
                        key_up(*vk, *ext);
                    }
                    return Err(format!("unknown key: {tok}"));
                }
            }
        }
    }
    for (vk, ext) in held.iter().rev() {
        key_up(*vk, *ext);
        pause();
    }
    Ok(())
}

/// Press or release a single mouse button (optionally moving to a point first),
/// for composing custom press-hold-release sequences.
pub fn mouse_button(action: &str, button: &str, at: Option<(i32, i32)>) -> Result<(), String> {
    crate::interrupt::mark_input();
    if let Some((x, y)) = at {
        move_smooth(x, y);
    }
    let (down, up) = button_flags(button);
    match action.trim().to_ascii_lowercase().as_str() {
        "down" | "press" => send(&[mouse_input(0, 0, 0, down)]),
        "up" | "release" => send(&[mouse_input(0, 0, 0, up)]),
        other => return Err(format!("mouse action must be down|up, got {other}")),
    }
    pause();
    Ok(())
}

/// Hold a set of keys down for `seconds`, then release in reverse order.
pub fn hold_keys(keys: &[String], seconds: f64) -> Result<(), String> {
    crate::interrupt::mark_input();
    let mut held: Vec<(u16, bool)> = Vec::new();
    for k in keys {
        match keysym::resolve(k) {
            Some((vk, ext)) => {
                key_down(vk, ext);
                held.push((vk, ext));
                pause();
            }
            None => {
                for (vk, ext) in held.iter().rev() {
                    key_up(*vk, *ext);
                }
                return Err(format!("unknown key: {k}"));
            }
        }
    }
    thread::sleep(Duration::from_secs_f64(seconds.clamp(0.0, 30.0)));
    for (vk, ext) in held.iter().rev() {
        key_up(*vk, *ext);
        pause();
    }
    Ok(())
}
