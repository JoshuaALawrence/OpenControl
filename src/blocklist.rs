use std::path::PathBuf;

use serde::Deserialize;

/// How a blocked window's pixels are obscured in captures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RedactMode {
    /// Fill the region with a solid RGB color.
    Solid([u8; 3]),
    /// Gaussian-blur the region with the given sigma (1..=200).
    Blur(f32),
}

impl Default for RedactMode {
    fn default() -> Self {
        RedactMode::Solid([0, 0, 0])
    }
}

/// A single user-defined rule. Every populated field must match (logical AND);
/// separate rules are OR-ed together. String fields are stored lowercased so
/// matching is case-insensitive.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockRule {
    /// Executable file name, e.g. `keepass.exe` (compared against the path's basename).
    pub exe_name: Option<String>,
    /// Substring of the full executable path.
    pub exe_path: Option<String>,
    /// Window title: substring match, or a `*` glob when the pattern contains `*`.
    pub title: Option<String>,
    /// Win32 window class name (exact, case-insensitive).
    pub class_name: Option<String>,
    /// Optional per-rule redaction mode; falls back to the list default.
    pub mode: Option<RedactMode>,
}

/// Facts about one window, used to evaluate rules. Strings are matched
/// case-insensitively; they are kept in their original case here.
#[derive(Debug, Clone, Default)]
pub struct WindowInfo {
    pub pid: u32,
    pub exe_path: Option<String>,
    pub title: Option<String>,
    pub class_name: String,
    /// (left, top, right, bottom) in physical screen pixels.
    pub rect: Option<(i32, i32, i32, i32)>,
}

impl BlockRule {
    /// True when every populated field of this rule matches `w`. A rule with no
    /// criteria never matches (so an empty rule can't block everything).
    pub fn matches(&self, w: &WindowInfo) -> bool {
        if self.is_empty() {
            return false;
        }
        if let Some(name) = &self.exe_name {
            let base = w.exe_path.as_deref().map(exe_basename).unwrap_or_default();
            if &base != name {
                return false;
            }
        }
        if let Some(path) = &self.exe_path {
            match &w.exe_path {
                Some(p) if p.to_lowercase().contains(path) => {}
                _ => return false,
            }
        }
        if let Some(t) = &self.title {
            match &w.title {
                Some(wt) if title_matches(&wt.to_lowercase(), t) => {}
                _ => return false,
            }
        }
        if let Some(c) = &self.class_name {
            if w.class_name.to_lowercase() != *c {
                return false;
            }
        }
        true
    }

    fn is_empty(&self) -> bool {
        self.exe_name.is_none()
            && self.exe_path.is_none()
            && self.title.is_none()
            && self.class_name.is_none()
    }
}

/// The user's full blocklist plus capture policy.
#[derive(Debug, Clone)]
pub struct Blocklist {
    pub rules: Vec<BlockRule>,
    /// When true, a capture is refused if window enumeration fails while rules
    /// are active (rather than risk leaking a blocked window).
    pub fail_closed: bool,
    /// Redaction style for rules that don't specify their own.
    pub default_mode: RedactMode,
}

impl Default for Blocklist {
    fn default() -> Self {
        Blocklist {
            rules: Vec::new(),
            fail_closed: true,
            default_mode: RedactMode::default(),
        }
    }
}

impl Blocklist {
    /// No rules — callers should skip all blocklist work entirely.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// True if any rule matches the window.
    pub fn is_blocked(&self, w: &WindowInfo) -> bool {
        self.rules.iter().any(|r| r.matches(w))
    }

    /// Redaction mode for a window: the first matching rule's explicit mode, or
    /// the list default. `None` if no rule matches.
    pub fn redact_mode_for(&self, w: &WindowInfo) -> Option<RedactMode> {
        self.rules
            .iter()
            .find(|r| r.matches(w))
            .map(|r| r.mode.unwrap_or(self.default_mode))
    }

    /// Parse a blocklist from JSON text. Pure and side-effect free.
    pub fn from_json_str(s: &str) -> Result<Blocklist, String> {
        let raw: RawBlocklist = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let default_mode = raw.default_mode.map(RawMode::compile).unwrap_or_default();
        let rules = raw.rules.into_iter().filter_map(RawRule::compile).collect();
        Ok(Blocklist {
            rules,
            fail_closed: raw.fail_closed.unwrap_or(true),
            default_mode,
        })
    }

    /// Append simple rules parsed from delimited environment-variable values
    /// (`;` or `,` separated). Pure helper used by [`Blocklist::load`] and tests.
    pub fn add_env_rules(&mut self, exe_list: &str, title_list: &str) {
        for name in split_list(exe_list) {
            self.rules.push(BlockRule {
                exe_name: Some(name),
                ..Default::default()
            });
        }
        for title in split_list(title_list) {
            self.rules.push(BlockRule {
                title: Some(title),
                ..Default::default()
            });
        }
    }

    /// Load the effective blocklist: the JSON config file (if present and valid)
    /// merged with environment-variable overrides. Never panics; an unreadable or
    /// invalid file is ignored (logged to stderr) so a broken config cannot
    /// silently disable the agent. Env vars still apply.
    pub fn load() -> Self {
        let mut bl = from_config_file().unwrap_or_default();
        let exe = std::env::var("OPENCONTROL_BLOCK_EXE").unwrap_or_default();
        let title = std::env::var("OPENCONTROL_BLOCK_TITLE").unwrap_or_default();
        bl.add_env_rules(&exe, &title);
        bl
    }
}

// ---- config file location --------------------------------------------------

fn config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("OPENCONTROL_BLOCKLIST") {
        let p = p.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let appdata = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(appdata)
            .join("OpenControl")
            .join("blocklist.json"),
    )
}

fn from_config_file() -> Option<Blocklist> {
    let path = config_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    match Blocklist::from_json_str(&text) {
        Ok(bl) => Some(bl),
        Err(e) => {
            eprintln!(
                "opencontrol: ignoring invalid blocklist {}: {e}",
                path.display()
            );
            None
        }
    }
}

// ---- matching helpers ------------------------------------------------------

/// Lowercased final path component of a Windows or Unix path.
fn exe_basename(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

/// Title match: plain substring when `pattern` has no `*`; otherwise an anchored
/// `*`-glob (each `*` matches any run of characters). `text` is pre-lowercased.
fn title_matches(text: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return text.contains(pattern);
    }
    glob_match(text.as_bytes(), pattern.as_bytes())
}

/// Iterative `*`-only wildcard match anchored to the whole string.
fn glob_match(text: &[u8], pat: &[u8]) -> bool {
    let (mut t, mut p) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while t < text.len() {
        if p < pat.len() && pat[p] == b'*' {
            star = p;
            mark = t;
            p += 1;
        } else if p < pat.len() && pat[p] == text[t] {
            p += 1;
            t += 1;
        } else if star != usize::MAX {
            p = star + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }
    p == pat.len()
}

fn split_list(s: &str) -> Vec<String> {
    s.split([';', ','])
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        6 => Some([
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        ]),
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()?;
            let g = u8::from_str_radix(&s[1..2], 16).ok()?;
            let b = u8::from_str_radix(&s[2..3], 16).ok()?;
            // Expand each nibble to a full byte (e.g. f -> ff).
            Some([r * 17, g * 17, b * 17])
        }
        _ => None,
    }
}

// ---- raw (deserializable) forms --------------------------------------------

#[derive(Debug, Deserialize)]
struct RawBlocklist {
    #[serde(default)]
    fail_closed: Option<bool>,
    #[serde(default)]
    default_mode: Option<RawMode>,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    #[serde(default)]
    exe_name: Option<String>,
    #[serde(default)]
    exe_path: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    class_name: Option<String>,
    #[serde(default)]
    mode: Option<RawMode>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RawMode {
    Solid {
        #[serde(default)]
        color: Option<String>,
    },
    Blur {
        #[serde(default)]
        sigma: Option<f32>,
    },
}

impl RawMode {
    fn compile(self) -> RedactMode {
        match self {
            RawMode::Solid { color } => {
                let rgb = color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or([0, 0, 0]);
                RedactMode::Solid(rgb)
            }
            RawMode::Blur { sigma } => RedactMode::Blur(sigma.unwrap_or(24.0).clamp(1.0, 200.0)),
        }
    }
}

impl RawRule {
    fn compile(self) -> Option<BlockRule> {
        let rule = BlockRule {
            exe_name: norm(self.exe_name),
            exe_path: norm(self.exe_path),
            title: norm(self.title),
            class_name: norm(self.class_name),
            mode: self.mode.map(RawMode::compile),
        };
        // Discard rules with no criteria so they can't match every window.
        if rule.exe_name.is_none()
            && rule.exe_path.is_none()
            && rule.title.is_none()
            && rule.class_name.is_none()
        {
            None
        } else {
            Some(rule)
        }
    }
}

/// Trim, lowercase, and drop empty strings.
fn norm(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(exe: &str, title: &str, class: &str) -> WindowInfo {
        WindowInfo {
            pid: 1,
            exe_path: Some(exe.to_string()),
            title: Some(title.to_string()),
            class_name: class.to_string(),
            rect: Some((0, 0, 100, 100)),
        }
    }

    #[test]
    fn exe_name_is_case_insensitive_basename() {
        let r = BlockRule {
            exe_name: Some("notepad.exe".into()),
            ..Default::default()
        };
        assert!(r.matches(&win(
            "C:\\Windows\\System32\\NOTEPAD.EXE",
            "Untitled",
            "Notepad"
        )));
        assert!(!r.matches(&win("C:\\Windows\\System32\\calc.exe", "Calc", "App")));
    }

    #[test]
    fn exe_path_is_substring() {
        let r = BlockRule {
            exe_path: Some("system32".into()),
            ..Default::default()
        };
        assert!(r.matches(&win("C:\\Windows\\System32\\notepad.exe", "x", "y")));
        assert!(!r.matches(&win("C:\\Apps\\notepad.exe", "x", "y")));
    }

    #[test]
    fn title_substring_and_wildcard() {
        let sub = BlockRule {
            title: Some("secret".into()),
            ..Default::default()
        };
        assert!(sub.matches(&win("a.exe", "My Secret Doc", "c")));
        assert!(!sub.matches(&win("a.exe", "Public Doc", "c")));

        let prefix = BlockRule {
            title: Some("secret*".into()),
            ..Default::default()
        };
        assert!(prefix.matches(&win("a.exe", "secret plans", "c")));
        assert!(!prefix.matches(&win("a.exe", "my secret", "c")));

        let suffix = BlockRule {
            title: Some("*vault".into()),
            ..Default::default()
        };
        assert!(suffix.matches(&win("a.exe", "password vault", "c")));
        assert!(!suffix.matches(&win("a.exe", "vault open", "c")));

        let mid = BlockRule {
            title: Some("bit*den".into()),
            ..Default::default()
        };
        assert!(mid.matches(&win("a.exe", "bitwarden", "c")));
        assert!(!mid.matches(&win("a.exe", "bitfoo", "c")));
    }

    #[test]
    fn class_name_exact_case_insensitive() {
        let r = BlockRule {
            class_name: Some("chrome_widgetwin_1".into()),
            ..Default::default()
        };
        assert!(r.matches(&win("a.exe", "t", "Chrome_WidgetWin_1")));
        assert!(!r.matches(&win("a.exe", "t", "Chrome_WidgetWin_2")));
    }

    #[test]
    fn and_within_rule() {
        let r = BlockRule {
            exe_name: Some("app.exe".into()),
            title: Some("private".into()),
            ..Default::default()
        };
        assert!(r.matches(&win("C:\\x\\app.exe", "private notes", "c")));
        // exe matches but title does not -> no match.
        assert!(!r.matches(&win("C:\\x\\app.exe", "public", "c")));
        // title matches but exe does not -> no match.
        assert!(!r.matches(&win("C:\\x\\other.exe", "private", "c")));
    }

    #[test]
    fn or_across_rules() {
        let bl = Blocklist {
            rules: vec![
                BlockRule {
                    exe_name: Some("a.exe".into()),
                    ..Default::default()
                },
                BlockRule {
                    exe_name: Some("b.exe".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(bl.is_blocked(&win("C:\\a.exe", "t", "c")));
        assert!(bl.is_blocked(&win("C:\\b.exe", "t", "c")));
        assert!(!bl.is_blocked(&win("C:\\c.exe", "t", "c")));
    }

    #[test]
    fn empty_rule_never_matches() {
        let r = BlockRule::default();
        assert!(!r.matches(&win("a.exe", "t", "c")));
    }

    #[test]
    fn empty_blocklist_blocks_nothing() {
        let bl = Blocklist::default();
        assert!(bl.is_empty());
        assert!(!bl.is_blocked(&win("a.exe", "t", "c")));
        assert_eq!(bl.redact_mode_for(&win("a.exe", "t", "c")), None);
    }

    #[test]
    fn redact_mode_uses_rule_then_default() {
        let bl = Blocklist {
            rules: vec![
                BlockRule {
                    exe_name: Some("blurme.exe".into()),
                    mode: Some(RedactMode::Blur(30.0)),
                    ..Default::default()
                },
                BlockRule {
                    exe_name: Some("default.exe".into()),
                    ..Default::default()
                },
            ],
            default_mode: RedactMode::Solid([1, 2, 3]),
            fail_closed: true,
        };
        assert_eq!(
            bl.redact_mode_for(&win("x\\blurme.exe", "t", "c")),
            Some(RedactMode::Blur(30.0))
        );
        assert_eq!(
            bl.redact_mode_for(&win("x\\default.exe", "t", "c")),
            Some(RedactMode::Solid([1, 2, 3]))
        );
    }

    #[test]
    fn json_parsing_full() {
        let json = r##"{
            "fail_closed": false,
            "default_mode": { "type": "solid", "color": "#102030" },
            "rules": [
                { "exe_name": "KeePass.exe" },
                { "title": "Vault", "mode": { "type": "blur", "sigma": 40 } },
                { "exe_path": "  ", "title": "  " }
            ]
        }"##;
        let bl = Blocklist::from_json_str(json).expect("parse");
        assert!(!bl.fail_closed);
        assert_eq!(bl.default_mode, RedactMode::Solid([0x10, 0x20, 0x30]));
        // The all-whitespace rule is dropped.
        assert_eq!(bl.rules.len(), 2);
        assert_eq!(bl.rules[0].exe_name.as_deref(), Some("keepass.exe"));
        assert_eq!(bl.rules[1].mode, Some(RedactMode::Blur(40.0)));
    }

    #[test]
    fn json_defaults_fail_closed_true() {
        let bl = Blocklist::from_json_str(r#"{ "rules": [] }"#).expect("parse");
        assert!(bl.fail_closed);
        assert!(bl.is_empty());
    }

    #[test]
    fn blur_sigma_is_clamped() {
        let bl = Blocklist::from_json_str(
            r#"{ "rules": [ { "title": "x", "mode": { "type": "blur", "sigma": 9000 } } ] }"#,
        )
        .expect("parse");
        assert_eq!(bl.rules[0].mode, Some(RedactMode::Blur(200.0)));
    }

    #[test]
    fn hex_color_parsing() {
        assert_eq!(parse_hex_color("#ff0000"), Some([255, 0, 0]));
        assert_eq!(parse_hex_color("00ff00"), Some([0, 255, 0]));
        assert_eq!(parse_hex_color("#f00"), Some([255, 0, 0]));
        assert_eq!(parse_hex_color("nope"), None);
    }

    #[test]
    fn env_rules_are_added() {
        let mut bl = Blocklist::default();
        bl.add_env_rules("KeePass.exe; bitwarden.exe", "Secret , Vault");
        assert_eq!(bl.rules.len(), 4);
        assert!(bl.is_blocked(&win("C:\\x\\keepass.exe", "t", "c")));
        assert!(bl.is_blocked(&win("C:\\x\\other.exe", "my vault", "c")));
        assert!(!bl.is_blocked(&win("C:\\x\\other.exe", "nothing", "c")));
    }
}
