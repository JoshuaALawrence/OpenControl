/// Resolve a single key token to a `(VK, extended)` pair.
///
/// Returns `None` for tokens that should be typed as a literal Unicode character
/// instead (handled by the caller via `VkKeyScan`/unicode injection).
pub fn resolve(token: &str) -> Option<(u16, bool)> {
    let t = token.trim();
    let lower = t.to_ascii_lowercase();
    // Direct named lookups (case-insensitive).
    if let Some(v) = named(&lower) {
        return Some(v);
    }
    // Single ASCII letter / digit.
    if t.chars().count() == 1 {
        let c = t.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Some((c.to_ascii_uppercase() as u16, false));
        }
        if c.is_ascii_digit() {
            return Some((c as u16, false));
        }
    }
    None
}

/// True when a token names a modifier key.
pub fn is_modifier(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "ctrl"
            | "control"
            | "control_l"
            | "control_r"
            | "lctrl"
            | "rctrl"
            | "alt"
            | "menu"
            | "alt_l"
            | "alt_r"
            | "lalt"
            | "ralt"
            | "shift"
            | "shift_l"
            | "shift_r"
            | "lshift"
            | "rshift"
            | "win"
            | "super"
            | "super_l"
            | "super_r"
            | "meta"
            | "meta_l"
            | "meta_r"
            | "cmd"
    )
}

fn named(k: &str) -> Option<(u16, bool)> {
    use windows::Win32::UI::Input::KeyboardAndMouse as kb;
    let v: (u16, bool) = match k {
        // modifiers
        "ctrl" | "control" => (kb::VK_CONTROL.0, false),
        "control_l" | "lctrl" | "leftctrl" => (kb::VK_LCONTROL.0, false),
        "control_r" | "rctrl" | "rightctrl" => (kb::VK_RCONTROL.0, true),
        "alt" | "menu" => (kb::VK_MENU.0, false),
        "alt_l" | "lalt" => (kb::VK_LMENU.0, false),
        "alt_r" | "ralt" => (kb::VK_RMENU.0, true),
        "shift" => (kb::VK_SHIFT.0, false),
        "shift_l" | "lshift" => (kb::VK_LSHIFT.0, false),
        "shift_r" | "rshift" => (kb::VK_RSHIFT.0, false),
        "win" | "super" | "super_l" | "meta" | "meta_l" | "cmd" | "lwin" => (kb::VK_LWIN.0, true),
        "super_r" | "meta_r" | "rwin" => (kb::VK_RWIN.0, true),
        "apps" | "menukey" => (kb::VK_APPS.0, true),
        // editing / navigation
        "enter" | "return" => (kb::VK_RETURN.0, false),
        "kp_enter" => (kb::VK_RETURN.0, true),
        "tab" => (kb::VK_TAB.0, false),
        "esc" | "escape" => (kb::VK_ESCAPE.0, false),
        "space" | "spacebar" => (kb::VK_SPACE.0, false),
        "backspace" | "back" | "bksp" => (kb::VK_BACK.0, false),
        "delete" | "del" => (kb::VK_DELETE.0, true),
        "insert" | "ins" => (kb::VK_INSERT.0, true),
        "home" => (kb::VK_HOME.0, true),
        "end" => (kb::VK_END.0, true),
        "pageup" | "pgup" | "prior" => (kb::VK_PRIOR.0, true),
        "pagedown" | "pgdn" | "next" => (kb::VK_NEXT.0, true),
        "left" => (kb::VK_LEFT.0, true),
        "up" => (kb::VK_UP.0, true),
        "right" => (kb::VK_RIGHT.0, true),
        "down" => (kb::VK_DOWN.0, true),
        // locks / system
        "capslock" => (kb::VK_CAPITAL.0, false),
        "numlock" => (kb::VK_NUMLOCK.0, false),
        "scrolllock" => (kb::VK_SCROLL.0, false),
        "printscreen" | "prtsc" | "prtscr" => (kb::VK_SNAPSHOT.0, true),
        "pause" | "break" => (kb::VK_PAUSE.0, false),
        // punctuation (X11 keysym names)
        "period" => (kb::VK_OEM_PERIOD.0, false),
        "comma" => (kb::VK_OEM_COMMA.0, false),
        "minus" => (kb::VK_OEM_MINUS.0, false),
        "equal" => (kb::VK_OEM_PLUS.0, false),
        "semicolon" => (kb::VK_OEM_1.0, false),
        "slash" => (kb::VK_OEM_2.0, false),
        "grave" => (kb::VK_OEM_3.0, false),
        "bracketleft" => (kb::VK_OEM_4.0, false),
        "backslash" => (kb::VK_OEM_5.0, false),
        "bracketright" => (kb::VK_OEM_6.0, false),
        "apostrophe" => (kb::VK_OEM_7.0, false),
        // numpad
        "kp_add" => (kb::VK_ADD.0, false),
        "kp_subtract" => (kb::VK_SUBTRACT.0, false),
        "kp_multiply" => (kb::VK_MULTIPLY.0, false),
        "kp_divide" => (kb::VK_DIVIDE.0, true),
        "kp_decimal" => (kb::VK_DECIMAL.0, false),
        _ => {
            // function keys F1..F24
            if let Some(rest) = k.strip_prefix('f') {
                if let Ok(n) = rest.parse::<u16>() {
                    if (1..=24).contains(&n) {
                        return Some((kb::VK_F1.0 + (n - 1), false));
                    }
                }
            }
            // numpad digits: kp_0..kp_9 / numpad_0..numpad_9
            for prefix in ["kp_", "numpad_"] {
                if let Some(rest) = k.strip_prefix(prefix) {
                    if let Ok(n) = rest.parse::<u16>() {
                        if n <= 9 {
                            return Some((kb::VK_NUMPAD0.0 + n, false));
                        }
                    }
                }
            }
            return None;
        }
    };
    Some(v)
}

/// A representative list of accepted key names (for discovery by the model).
pub fn all_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "ctrl",
        "control",
        "control_l",
        "control_r",
        "alt",
        "alt_l",
        "alt_r",
        "shift",
        "shift_l",
        "shift_r",
        "win",
        "super",
        "super_l",
        "super_r",
        "meta",
        "apps",
        "enter",
        "return",
        "kp_enter",
        "tab",
        "esc",
        "escape",
        "space",
        "backspace",
        "delete",
        "insert",
        "home",
        "end",
        "pageup",
        "prior",
        "pagedown",
        "next",
        "left",
        "up",
        "right",
        "down",
        "capslock",
        "numlock",
        "scrolllock",
        "printscreen",
        "pause",
        "period",
        "comma",
        "minus",
        "equal",
        "semicolon",
        "slash",
        "grave",
        "bracketleft",
        "backslash",
        "bracketright",
        "apostrophe",
        "kp_add",
        "kp_subtract",
        "kp_multiply",
        "kp_divide",
        "kp_decimal",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for n in 1..=24 {
        names.push(format!("f{n}"));
    }
    for d in 0..=9 {
        names.push(format!("{d}"));
        names.push(format!("kp_{d}"));
    }
    for c in b'a'..=b'z' {
        names.push((c as char).to_string());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_modifier_keys() {
        // Test control key variants
        assert!(resolve("ctrl").is_some());
        assert!(resolve("control").is_some());
        assert!(resolve("control_l").is_some());
        assert!(resolve("control_r").is_some());

        // Test shift variants
        assert!(resolve("shift").is_some());
        assert!(resolve("shift_l").is_some());
        assert!(resolve("shift_r").is_some());

        // Test alt variants
        assert!(resolve("alt").is_some());
        assert!(resolve("alt_l").is_some());
        assert!(resolve("alt_r").is_some());

        // Test win/super variants
        assert!(resolve("win").is_some());
        assert!(resolve("super").is_some());
    }

    #[test]
    fn test_resolve_function_keys() {
        // Test F1..F24
        assert!(resolve("f1").is_some());
        assert!(resolve("f12").is_some());
        assert!(resolve("f24").is_some());
        assert!(resolve("f25").is_none());
    }

    #[test]
    fn test_resolve_navigation_keys() {
        assert!(resolve("enter").is_some());
        assert!(resolve("return").is_some());
        assert!(resolve("tab").is_some());
        assert!(resolve("escape").is_some());
        assert!(resolve("home").is_some());
        assert!(resolve("end").is_some());
        assert!(resolve("pageup").is_some());
        assert!(resolve("pagedown").is_some());
        assert!(resolve("left").is_some());
        assert!(resolve("right").is_some());
        assert!(resolve("up").is_some());
        assert!(resolve("down").is_some());
    }

    #[test]
    fn test_resolve_ascii_letters() {
        assert!(resolve("a").is_some());
        assert!(resolve("z").is_some());
        assert!(resolve("A").is_some());
    }

    #[test]
    fn test_resolve_ascii_digits() {
        assert!(resolve("0").is_some());
        assert!(resolve("9").is_some());
    }

    #[test]
    fn test_resolve_numpad_keys() {
        assert!(resolve("kp_0").is_some());
        assert!(resolve("kp_9").is_some());
        assert!(resolve("numpad_0").is_some());
        assert!(resolve("kp_add").is_some());
    }

    #[test]
    fn test_is_modifier() {
        assert!(is_modifier("ctrl"));
        assert!(is_modifier("alt"));
        assert!(is_modifier("shift"));
        assert!(is_modifier("win"));
        assert!(!is_modifier("enter"));
        assert!(!is_modifier("unknown"));
    }

    #[test]
    fn test_all_names_completeness() {
        let names = all_names();
        assert!(!names.is_empty());
        assert!(names.contains(&"ctrl".to_string()));
        assert!(names.contains(&"enter".to_string()));
        assert!(names.contains(&"f1".to_string()));
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"0".to_string()));
    }

    #[test]
    fn test_case_insensitivity() {
        assert_eq!(resolve("ctrl"), resolve("CTRL"));
        assert_eq!(resolve("enter"), resolve("ENTER"));
        assert_eq!(resolve("F1"), resolve("f1"));
    }
}
