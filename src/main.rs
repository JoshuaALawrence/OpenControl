use opencontrol::{
    blocklist, capture, desktop, input, installed, interrupt, ocr, redact, sys, uia, vision,
    winutil, worker::Worker,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorCode;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::sync::Arc;

const MAX_ELEMENTS: usize = 400;
const MAX_DEPTH: usize = 60;
const SCREENSHOT_MAX_DIM: u32 = 1568;
const JPEG_QUALITY: u8 = 82;

const INSTRUCTIONS: &str = "Full control of a Windows computer (all-native). Core loop:\n\
1. list_windows to find a target window, then observe(window_handle) for ONE call that returns a \
screenshot with numbered boxes over controls (Set-of-Marks) PLUS a compact accessibility tree where \
every control has an [index] and its supported actions in {braces}.\n\
2. Act by index: click_element / set_element_value / invoke_element with that index - most reliable.\n\
3. Fall back to take_screenshot + coordinate click/drag for controls the tree doesn't expose. Aim for \
the CENTER of elements. If a click doesn't register, observe again and use an index.\n\
4. Built-in OCR (ocr / find_text) reads on-screen text with clickable boxes - no install needed.\n\
Press the physical Escape key to stop automation at any time.";

#[derive(Clone)]
struct Cu {
    worker: Worker,
    // Used by the #[tool_handler] macro for dispatch; not read directly.
    #[allow(dead_code)]
    tool_router: ToolRouter<Cu>,
    // User-defined application blocklist (redaction + access control). Loaded
    // once at startup and never mutated by the AI.
    blocklist: Arc<blocklist::Blocklist>,
}

// ---- result helpers --------------------------------------------------------
fn mcp_err(msg: impl Into<String>) -> McpError {
    McpError {
        code: ErrorCode(-32603),
        message: Cow::from(msg.into()),
        data: None,
    }
}
/// Error returned when a tool targets an application the user has blocked.
fn blocked_err() -> McpError {
    mcp_err(
        "target application is blocked by the user's OpenControl blocklist and cannot be \
         seen or controlled",
    )
}
fn text_ok(s: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(s.into())]))
}
fn json_ok(v: &Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()),
    )]))
}
fn image_ok(text: String, b64: String, mime: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![
        Content::text(text),
        Content::image(b64, mime),
    ]))
}
fn guard() -> Result<(), McpError> {
    if interrupt::is_interrupted() {
        // Consume the interrupt: abort this action and signal the agent to stop,
        // but don't permanently lock out future turns (no per-turn reset here).
        interrupt::clear_interrupt();
        return Err(mcp_err(interrupt::INTERRUPT_MSG));
    }
    Ok(())
}

// ---- parameter structs -----------------------------------------------------
// Lenient parameter parsing. MCP hosts/models frequently send numbers as
// strings ("106"), as floats (106.0), or even cram a whole coordinate pair into
// one field ("106, 745"). These helpers coerce those shapes into the expected
// scalar types instead of failing the entire tool call with a -32602 error.
mod flex {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer};
    use serde_json::{Map, Value};

    /// Leading (optionally signed) integer in a string, ignoring junk and any
    /// trailing characters: "106", " 106 ", "106, 745", "x=106.5" -> 106.
    fn parse_first_int(s: &str) -> Option<i64> {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i] != b'-' && b[i] != b'+' && !b[i].is_ascii_digit() {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        let start = i;
        if b[i] == b'-' || b[i] == b'+' {
            i += 1;
        }
        let digits = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == digits {
            return None;
        }
        s[start..i].parse::<i64>().ok()
    }

    /// Every integer in a string: "0, 0, 800, 600" -> [0, 0, 800, 600].
    fn parse_int_list(s: &str) -> Vec<i64> {
        s.split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c == '.'))
            .filter(|t| !t.is_empty())
            .filter_map(parse_first_int)
            .collect()
    }

    /// Coerce a JSON value (number, float, bool, or numeric string) to i64.
    pub fn value_to_i64(v: &Value) -> Option<i64> {
        match v {
            Value::Number(n) => n
                .as_i64()
                .or_else(|| n.as_u64().map(|u| u as i64))
                .or_else(|| n.as_f64().map(|f| f.round() as i64)),
            Value::String(s) => parse_first_int(s),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    /// All integers carried by a value (array elements, or numbers in a string).
    fn collect_ints(v: &Value) -> Vec<i64> {
        match v {
            Value::Array(a) => a.iter().filter_map(value_to_i64).collect(),
            Value::String(s) => parse_int_list(s),
            _ => value_to_i64(v).into_iter().collect(),
        }
    }

    /// Deserialize the arguments into a plain JSON object map.
    pub fn object<'de, D>(d: D) -> Result<Map<String, Value>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(d)? {
            Value::Object(m) => Ok(m),
            other => Err(D::Error::custom(format!(
                "expected an object, got `{other}`"
            ))),
        }
    }

    /// Resolve an ordered set of coordinate fields, tolerating a single field
    /// that carries them all (e.g. x = "106, 745" or x = [106, 745]).
    pub fn coords<E: Error>(m: &Map<String, Value>, keys: &[&str]) -> Result<Vec<i32>, E> {
        let present: Vec<&Value> = keys
            .iter()
            .filter_map(|k| m.get(*k).filter(|v| !v.is_null()))
            .collect();
        if present.len() == 1 {
            let nums = collect_ints(present[0]);
            if nums.len() >= keys.len() {
                return Ok(nums[..keys.len()].iter().map(|n| *n as i32).collect());
            }
        }
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let v = m
                .get(*k)
                .filter(|v| !v.is_null())
                .ok_or_else(|| E::custom(format!("missing field `{k}`")))?;
            let n = value_to_i64(v)
                .ok_or_else(|| E::custom(format!("field `{k}` must be an integer, got `{v}`")))?;
            out.push(n as i32);
        }
        Ok(out)
    }

    pub fn req_i64<E: Error>(m: &Map<String, Value>, k: &str) -> Result<i64, E> {
        let v = m
            .get(k)
            .filter(|v| !v.is_null())
            .ok_or_else(|| E::custom(format!("missing field `{k}`")))?;
        value_to_i64(v)
            .ok_or_else(|| E::custom(format!("field `{k}` must be an integer, got `{v}`")))
    }

    pub fn req_string<E: Error>(m: &Map<String, Value>, k: &str) -> Result<String, E> {
        m.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| E::custom(format!("missing string field `{k}`")))
    }

    pub fn opt_string(m: &Map<String, Value>, k: &str) -> Option<String> {
        m.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
    }
    pub fn opt_i32(m: &Map<String, Value>, k: &str) -> Option<i32> {
        m.get(k).and_then(value_to_i64).map(|n| n as i32)
    }
    pub fn opt_u32(m: &Map<String, Value>, k: &str) -> Option<u32> {
        m.get(k).and_then(value_to_i64).map(|n| n.max(0) as u32)
    }

    // ---- deserialize_with adapters for fields that keep #[derive(Deserialize)].
    pub fn de_i64<'de, D>(d: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        value_to_i64(&v).ok_or_else(|| D::Error::custom(format!("expected an integer, got `{v}`")))
    }
    pub fn de_i64_opt<'de, D>(d: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        if v.is_null() {
            return Ok(None);
        }
        value_to_i64(&v)
            .map(Some)
            .ok_or_else(|| D::Error::custom(format!("expected an integer, got `{v}`")))
    }
    pub fn de_f64<'de, D>(d: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        match &v {
            Value::Number(n) => n.as_f64().ok_or_else(|| D::Error::custom("invalid number")),
            Value::String(s) => s
                .trim()
                .parse::<f64>()
                .map_err(|_| D::Error::custom(format!("expected a number, got `{v}`"))),
            _ => Err(D::Error::custom(format!("expected a number, got `{v}`"))),
        }
    }
    pub fn de_u32_opt<'de, D>(d: D) -> Result<Option<u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        if v.is_null() {
            return Ok(None);
        }
        value_to_i64(&v)
            .map(|n| Some(n.max(0) as u32))
            .ok_or_else(|| D::Error::custom(format!("expected an integer, got `{v}`")))
    }
    pub fn de_u64_opt<'de, D>(d: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        if v.is_null() {
            return Ok(None);
        }
        value_to_i64(&v)
            .map(|n| Some(n.max(0) as u64))
            .ok_or_else(|| D::Error::custom(format!("expected an integer, got `{v}`")))
    }
    pub fn de_usize<'de, D>(d: D) -> Result<usize, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        value_to_i64(&v)
            .map(|n| n.max(0) as usize)
            .ok_or_else(|| D::Error::custom(format!("expected a non-negative integer, got `{v}`")))
    }
    pub fn de_usize_opt<'de, D>(d: D) -> Result<Option<usize>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(d)?;
        if v.is_null() {
            return Ok(None);
        }
        value_to_i64(&v)
            .map(|n| Some(n.max(0) as usize))
            .ok_or_else(|| D::Error::custom(format!("expected a non-negative integer, got `{v}`")))
    }
    pub fn de_vec_i32_opt<'de, D>(d: D) -> Result<Option<Vec<i32>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(d)? {
            Value::Null => Ok(None),
            Value::Array(a) => {
                let mut out = Vec::new();
                for it in &a {
                    if let Some(n) = value_to_i64(it) {
                        out.push(n as i32);
                    } else if let Value::String(s) = it {
                        out.extend(parse_int_list(s).into_iter().map(|n| n as i32));
                    }
                }
                Ok(Some(out))
            }
            Value::String(s) => Ok(Some(
                parse_int_list(&s).into_iter().map(|n| n as i32).collect(),
            )),
            other => value_to_i64(&other)
                .map(|n| Some(vec![n as i32]))
                .ok_or_else(|| D::Error::custom(format!("expected an array, got `{other}`"))),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ScreenshotParams {
    #[schemars(description = "Monitor index (1-based). Omit for the primary monitor.")]
    #[serde(default, deserialize_with = "flex::de_u32_opt")]
    monitor: Option<u32>,
    #[schemars(description = "Absolute desktop region [x, y, width, height]. Overrides monitor.")]
    #[serde(default, deserialize_with = "flex::de_vec_i32_opt")]
    region: Option<Vec<i32>>,
    #[schemars(description = "Capture the whole virtual desktop (all monitors).")]
    full_virtual: Option<bool>,
    #[schemars(description = "Overlay a coordinate grid labelled in image pixels.")]
    grid: Option<bool>,
    #[schemars(description = "Draw the mouse cursor position.")]
    show_cursor: Option<bool>,
    #[schemars(description = "Longest-edge cap in px (default 1568; 0 = native).")]
    #[serde(default, deserialize_with = "flex::de_u32_opt")]
    max_dimension: Option<u32>,
    #[schemars(description = "'jpeg' (default) or 'png'.")]
    image_format: Option<String>,
}

#[derive(Debug, JsonSchema)]
struct PointParams {
    x: i32,
    y: i32,
}
impl<'de> Deserialize<'de> for PointParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let c = flex::coords::<D::Error>(&m, &["x", "y"])?;
        Ok(Self { x: c[0], y: c[1] })
    }
}

#[derive(Debug, JsonSchema)]
struct ClickParams {
    x: i32,
    y: i32,
    #[schemars(description = "left|right|middle (default left).")]
    button: Option<String>,
    #[schemars(description = "Number of clicks (1=single, 2=double, 3=triple).")]
    clicks: Option<u32>,
}
impl<'de> Deserialize<'de> for ClickParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let c = flex::coords::<D::Error>(&m, &["x", "y"])?;
        Ok(Self {
            x: c[0],
            y: c[1],
            button: flex::opt_string(&m, "button"),
            clicks: flex::opt_u32(&m, "clicks"),
        })
    }
}

#[derive(Debug, JsonSchema)]
struct DragParams {
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    button: Option<String>,
}
impl<'de> Deserialize<'de> for DragParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let c = flex::coords::<D::Error>(&m, &["start_x", "start_y", "end_x", "end_y"])?;
        Ok(Self {
            start_x: c[0],
            start_y: c[1],
            end_x: c[2],
            end_y: c[3],
            button: flex::opt_string(&m, "button"),
        })
    }
}

#[derive(Debug, JsonSchema)]
struct ScrollParams {
    #[schemars(description = "Image-space point to scroll over.")]
    x: i32,
    y: i32,
    #[schemars(description = "Horizontal notches (negative=left).")]
    scroll_x: Option<i32>,
    #[schemars(description = "Vertical notches (negative=up, positive=down).")]
    scroll_y: Option<i32>,
}
impl<'de> Deserialize<'de> for ScrollParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let c = flex::coords::<D::Error>(&m, &["x", "y"])?;
        Ok(Self {
            x: c[0],
            y: c[1],
            scroll_x: flex::opt_i32(&m, "scroll_x"),
            scroll_y: flex::opt_i32(&m, "scroll_y"),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TypeTextParams {
    text: String,
    #[schemars(description = "Per-character delay in ms for apps that drop fast input.")]
    #[serde(default, deserialize_with = "flex::de_u64_opt")]
    per_key_delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PressKeyParams {
    #[schemars(
        description = "'+'-separated chord using keysym names, e.g. 'Control_L+a', 'Return', 'Tab', 'KP_0'."
    )]
    keys: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HandleParams {
    #[serde(deserialize_with = "flex::de_i64")]
    window_handle: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WindowStateParams {
    #[serde(deserialize_with = "flex::de_i64")]
    window_handle: i64,
    #[schemars(description = "minimize | maximize | restore.")]
    state: String,
}

#[derive(Debug, JsonSchema)]
struct MoveResizeParams {
    window_handle: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
impl<'de> Deserialize<'de> for MoveResizeParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let window_handle = flex::req_i64::<D::Error>(&m, "window_handle")?;
        let c = flex::coords::<D::Error>(&m, &["x", "y", "width", "height"])?;
        Ok(Self {
            window_handle,
            x: c[0],
            y: c[1],
            width: c[2],
            height: c[3],
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LaunchParams {
    #[schemars(
        description = "Executable path or shell identifier (e.g. 'notepad', 'calc.exe', 'ms-settings:')."
    )]
    app: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PowershellParams {
    script: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SetClipboardParams {
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReadFileParams {
    path: String,
    #[serde(default, deserialize_with = "flex::de_usize_opt")]
    max_bytes: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WriteFileParams {
    path: String,
    content: String,
    overwrite: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PathParams {
    path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WaitParams {
    #[serde(deserialize_with = "flex::de_f64")]
    seconds: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ObserveParams {
    #[schemars(
        description = "Window handle from list_windows. Omit to use the foreground window."
    )]
    #[serde(default, deserialize_with = "flex::de_i64_opt")]
    window_handle: Option<i64>,
    #[schemars(description = "Include the accessibility tree (default true).")]
    include_text: Option<bool>,
    #[schemars(description = "Draw numbered Set-of-Marks on the screenshot (default true).")]
    marks: Option<bool>,
    #[schemars(description = "'concise' (default) or 'detailed' (adds automation ids).")]
    response_format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ElementClickParams {
    #[schemars(description = "Element index from the latest observe of this window.")]
    #[serde(deserialize_with = "flex::de_usize")]
    element_index: usize,
    button: Option<String>,
    #[schemars(description = "Double-click when true.")]
    double: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SetValueParams {
    #[serde(deserialize_with = "flex::de_usize")]
    element_index: usize,
    value: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct InvokeParams {
    #[serde(deserialize_with = "flex::de_usize")]
    element_index: usize,
    #[schemars(
        description = "auto|invoke|toggle|expand|collapse|select|scroll up|scroll down|scroll into view|realize|range N|..."
    )]
    action: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct OcrParams {
    #[schemars(
        description = "Absolute region [x, y, width, height]. Omit for the whole virtual desktop."
    )]
    #[serde(default, deserialize_with = "flex::de_vec_i32_opt")]
    region: Option<Vec<i32>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FindTextParams {
    #[schemars(description = "Substring to find (case-insensitive).")]
    text: String,
    #[serde(default, deserialize_with = "flex::de_vec_i32_opt")]
    region: Option<Vec<i32>>,
}

#[derive(Debug, JsonSchema)]
struct ZoomParams {
    #[schemars(description = "Image-space region to magnify: x, y, width, height.")]
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
impl<'de> Deserialize<'de> for ZoomParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let c = flex::coords::<D::Error>(&m, &["x", "y", "width", "height"])?;
        Ok(Self {
            x: c[0],
            y: c[1],
            width: c[2],
            height: c[3],
        })
    }
}

#[derive(Debug, JsonSchema)]
struct MouseButtonParams {
    #[schemars(description = "down|up.")]
    action: String,
    #[schemars(description = "left|right|middle (default left).")]
    button: Option<String>,
    #[schemars(description = "Optional image-space point to move to first.")]
    x: Option<i32>,
    y: Option<i32>,
}
impl<'de> Deserialize<'de> for MouseButtonParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = flex::object(d)?;
        let action = flex::req_string::<D::Error>(&m, "action")?;
        let button = flex::opt_string(&m, "button");
        let has_x = m.get("x").is_some_and(|v| !v.is_null());
        let has_y = m.get("y").is_some_and(|v| !v.is_null());
        let (x, y) = if has_x || has_y {
            let c = flex::coords::<D::Error>(&m, &["x", "y"])?;
            (Some(c[0]), Some(c[1]))
        } else {
            (None, None)
        };
        Ok(Self {
            action,
            button,
            x,
            y,
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KeySequenceParams {
    #[schemars(
        description = "List of '+'-separated chords, pressed in order (e.g. ['alt+f','s'])."
    )]
    chords: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HoldKeyParams {
    #[schemars(description = "Keys to hold simultaneously (keysym names).")]
    keys: Vec<String>,
    #[schemars(description = "How long to hold, seconds (max 30).")]
    #[serde(deserialize_with = "flex::de_f64")]
    seconds: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TopmostParams {
    #[serde(deserialize_with = "flex::de_i64")]
    window_handle: i64,
    #[schemars(description = "true to pin above all windows, false to unpin.")]
    enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RunCommandParams {
    #[schemars(description = "Executable to run.")]
    program: String,
    #[schemars(description = "Arguments.")]
    args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListProcessesParams {
    #[schemars(description = "Optional case-insensitive name substring filter.")]
    filter: Option<String>,
    #[schemars(description = "Max processes to return (default 100).")]
    #[serde(default, deserialize_with = "flex::de_usize_opt")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KillProcessParams {
    #[schemars(description = "Process id to kill.")]
    #[serde(default, deserialize_with = "flex::de_u32_opt")]
    pid: Option<u32>,
    #[schemars(description = "Or exe name to kill (all matching).")]
    name: Option<String>,
    force: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UiTreeParams {
    #[schemars(description = "Window handle to dump. Omit for the whole desktop.")]
    #[serde(default, deserialize_with = "flex::de_i64_opt")]
    window_handle: Option<i64>,
    #[schemars(description = "'concise' (default) or 'detailed' (adds automation ids).")]
    response_format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FindElementsParams {
    #[serde(deserialize_with = "flex::de_i64")]
    window_handle: i64,
    #[schemars(description = "Name substring to match (case-insensitive).")]
    name: Option<String>,
    #[schemars(description = "Control type substring, e.g. Button, Edit, MenuItem.")]
    control_type: Option<String>,
    #[schemars(description = "Max matches (default 30).")]
    #[serde(default, deserialize_with = "flex::de_usize_opt")]
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FindImageParams {
    #[schemars(description = "Path to the template image (png/jpg) to locate.")]
    template_path: String,
    #[schemars(
        description = "Absolute search region [x, y, width, height]. Omit for whole desktop (slower)."
    )]
    #[serde(default, deserialize_with = "flex::de_vec_i32_opt")]
    region: Option<Vec<i32>>,
    #[schemars(description = "Match threshold 0..1 (default 0.9).")]
    threshold: Option<f32>,
    #[schemars(description = "Return all matches above threshold (default false = best only).")]
    all_matches: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SaveScreenshotParams {
    #[schemars(description = "Destination .png path.")]
    path: String,
    #[schemars(
        description = "Absolute region [x, y, width, height]. Omit for the whole virtual desktop."
    )]
    #[serde(default, deserialize_with = "flex::de_vec_i32_opt")]
    region: Option<Vec<i32>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PasteParams {
    #[schemars(description = "Optional text to place on the clipboard before pasting.")]
    text: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WaitChangeParams {
    #[schemars(description = "Max seconds to wait (default 10, max 120).")]
    timeout: Option<f64>,
    #[schemars(description = "Poll interval seconds (default 0.4).")]
    poll_interval: Option<f64>,
    #[schemars(description = "Fraction of the screen that must change to count (default 0.01).")]
    threshold: Option<f64>,
    #[schemars(description = "Optional window handle to watch instead of the primary monitor.")]
    window_handle: Option<i64>,
    #[schemars(description = "screen (default), foreground, or window.")]
    scope: Option<String>,
}

// ---- tools -----------------------------------------------------------------
#[tool_router]
impl Cu {
    fn new() -> Self {
        Self {
            worker: Worker::new(),
            tool_router: Self::tool_router(),
            blocklist: Arc::new(blocklist::Blocklist::load()),
        }
    }

    /// Run a closure on the automation thread, first refusing if the target
    /// window matches the user blocklist. Used by every window-targeting tool.
    async fn guard_hwnd<R, F>(&self, handle: i64, f: F) -> Result<R, McpError>
    where
        R: Send + 'static,
        F: FnOnce() -> R + Send + 'static,
    {
        let bl = self.blocklist.clone();
        self.worker
            .run(move || {
                let hwnd = winutil::hwnd_from_id(handle);
                if !bl.is_empty() && bl.is_blocked(&winutil::window_info(hwnd)) {
                    return Err(blocked_err());
                }
                Ok(f())
            })
            .await
    }

    fn region_of(region: &Option<Vec<i32>>) -> Option<(i32, i32, i32, i32)> {
        region.as_ref().and_then(|r| {
            if r.len() == 4 {
                Some((r[0], r[1], r[2], r[3]))
            } else {
                None
            }
        })
    }

    #[tool(description = "List monitor layout and the virtual-desktop bounds.")]
    async fn screen_info(&self) -> Result<CallToolResult, McpError> {
        let v = self.worker.run(desktop::list_monitors).await;
        json_ok(&v)
    }

    #[tool(
        description = "Capture the screen (primary monitor by default) and return it as an image. \
Pixel coords (0,0 = top-left of the returned image) are what you pass to click/move/drag. \
Optionally target a monitor, an absolute region, the whole virtual desktop, a coordinate grid, or the cursor."
    )]
    async fn take_screenshot(
        &self,
        Parameters(p): Parameters<ScreenshotParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let region = Self::region_of(&p.region);
        let monitor = p.monitor.unwrap_or(0) as usize;
        let full_virtual = p.full_virtual.unwrap_or(false);
        let grid = p.grid.unwrap_or(false);
        let show_cursor = p.show_cursor.unwrap_or(false);
        let max_dim = p.max_dimension.unwrap_or(SCREENSHOT_MAX_DIM);
        let fmt = p.image_format.unwrap_or_else(|| "jpeg".into());

        let bl = self.blocklist.clone();
        let out = self
            .worker
            .run(move || -> Result<(String, String, String), String> {
                let mut bm = if let Some((x, y, w, h)) = region {
                    desktop::capture_region(x, y, w, h)?
                } else if full_virtual {
                    desktop::capture_virtual()?
                } else {
                    desktop::capture_monitor(monitor)?
                };
                redact::apply_to_bitmap(&mut bm, &bl)?;
                let img = desktop::bitmap_to_image(&bm).ok_or("failed to wrap pixels")?;
                let (mut img, scale) = desktop::scale_to(img, max_dim);
                if grid {
                    desktop::annotate_grid(&mut img, 100);
                }
                if show_cursor {
                    let (cx, cy) = desktop::cursor_pos();
                    let ix = ((cx - bm.origin_x) as f64 * scale).round() as i32;
                    let iy = ((cy - bm.origin_y) as f64 * scale).round() as i32;
                    desktop::annotate_cursor(&mut img, ix, iy);
                }
                desktop::set_view(bm.origin_x, bm.origin_y, scale);
                let (b64, mime) = desktop::encode(&img, &fmt, JPEG_QUALITY)?;
                let meta = json!({
                    "image_width": img.width(),
                    "image_height": img.height(),
                    "origin_x": bm.origin_x,
                    "origin_y": bm.origin_y,
                    "scale": scale,
                    "coordinate_space": "image pixels (0,0 top-left); pass straight to click/move/drag"
                })
                .to_string();
                Ok((meta, b64, mime))
            })
            .await
            .map_err(mcp_err)?;
        image_ok(format!("Screenshot captured.\n{}", out.0), out.1, out.2)
    }

    #[tool(description = "Return the current mouse cursor position in physical screen pixels.")]
    async fn get_cursor_position(&self) -> Result<CallToolResult, McpError> {
        let (x, y) = self.worker.run(desktop::cursor_pos).await;
        json_ok(&json!({ "x": x, "y": y }))
    }

    #[tool(
        description = "Read the RGB color of a pixel at image-space (x, y) from the last screenshot."
    )]
    async fn get_pixel_color(
        &self,
        Parameters(p): Parameters<PointParams>,
    ) -> Result<CallToolResult, McpError> {
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                let bm = desktop::capture_region(sx, sy, 1, 1)?;
                let (r, g, b) = (bm.rgba[0], bm.rgba[1], bm.rgba[2]);
                Ok(json!({
                    "x": sx, "y": sy, "rgb": [r, g, b],
                    "hex": format!("#{:02x}{:02x}{:02x}", r, g, b)
                }))
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(description = "Move the mouse cursor to image-space (x, y) without clicking.")]
    async fn move_mouse(
        &self,
        Parameters(p): Parameters<PointParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        self.worker.run(move || input::move_smooth(sx, sy)).await;
        text_ok(format!("Moved cursor to image ({},{}).", p.x, p.y))
    }

    #[tool(
        description = "Click at image-space (x, y). button=left|right|middle, clicks=1|2|3. \
Coordinates come from the most recent screenshot/observe."
    )]
    async fn click(
        &self,
        Parameters(p): Parameters<ClickParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        let button = p.button.unwrap_or_else(|| "left".into());
        let clicks = p.clicks.unwrap_or(1).max(1);
        self.worker
            .run(move || input::click(sx, sy, &button, clicks))
            .await;
        text_ok(format!("Clicked at image ({},{}).", p.x, p.y))
    }

    #[tool(
        description = "Press the mouse at the start point, drag to the end point, and release. \
Image-space coordinates. Use for sliders, selections, drag-and-drop."
    )]
    async fn drag(
        &self,
        Parameters(p): Parameters<DragParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let from = desktop::to_screen(p.start_x, p.start_y);
        let to = desktop::to_screen(p.end_x, p.end_y);
        let button = p.button.unwrap_or_else(|| "left".into());
        self.worker
            .run(move || input::drag(from, to, &button))
            .await;
        text_ok(format!(
            "Dragged from ({},{}) to ({},{}).",
            p.start_x, p.start_y, p.end_x, p.end_y
        ))
    }

    #[tool(
        description = "Scroll the wheel over image-space (x, y). scroll_y negative=up, positive=down; scroll_x for horizontal."
    )]
    async fn scroll(
        &self,
        Parameters(p): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        let dx = p.scroll_x.unwrap_or(0);
        let dy = p.scroll_y.unwrap_or(0);
        self.worker.run(move || input::scroll(sx, sy, dx, dy)).await;
        text_ok(format!(
            "Scrolled ({},{}) at image ({},{}).",
            dx, dy, p.x, p.y
        ))
    }

    #[tool(
        description = "Type Unicode text into the focused control. Set per_key_delay_ms for apps that drop fast input."
    )]
    async fn type_text(
        &self,
        Parameters(p): Parameters<TypeTextParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let text = p.text.clone();
        let delay = p.per_key_delay_ms.unwrap_or(0);
        let n = text.chars().count();
        self.worker
            .run(move || input::type_text_paced(&text, delay))
            .await;
        text_ok(format!("Typed {n} characters."))
    }

    #[tool(
        description = "Press a '+'-separated key chord using keysym names (e.g. 'Control_L+a', 'Return', 'Alt_L+F4')."
    )]
    async fn press_key(
        &self,
        Parameters(p): Parameters<PressKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let keys = p.keys.clone();
        self.worker
            .run(move || input::press_chord(&keys))
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Pressed {}.", p.keys))
    }

    #[tool(
        description = "List targetable top-level windows: handle (id), title and owning app path."
    )]
    async fn list_windows(&self) -> Result<CallToolResult, McpError> {
        let bl = self.blocklist.clone();
        let wins = self
            .worker
            .run(move || {
                winutil::enum_top_level()
                    .into_iter()
                    .filter(|&h| bl.is_empty() || !bl.is_blocked(&winutil::window_info(h)))
                    .map(winutil::to_window)
                    .collect::<Vec<_>>()
            })
            .await;
        json_ok(&json!({ "count": wins.len(), "windows": wins }))
    }

    #[tool(
        description = "List installed apps (from Start Menu shortcuts) with running state and any open windows."
    )]
    async fn list_apps(&self) -> Result<CallToolResult, McpError> {
        let bl = self.blocklist.clone();
        let apps = self
            .worker
            .run(move || {
                let mut apps = installed::list_installed_apps();
                if !bl.is_empty() {
                    apps.retain(|a| {
                        let info = blocklist::WindowInfo {
                            exe_path: Some(a.id.clone()),
                            ..Default::default()
                        };
                        !bl.is_blocked(&info)
                    });
                    for a in &mut apps {
                        a.windows.retain(|w| {
                            !bl.is_blocked(&winutil::window_info(winutil::hwnd_from_id(w.id)))
                        });
                    }
                }
                apps
            })
            .await;
        json_ok(&json!({ "count": apps.len(), "apps": apps }))
    }

    #[tool(description = "Return the active (foreground) window's handle, title and app.")]
    async fn get_active_window(&self) -> Result<CallToolResult, McpError> {
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                let hwnd = winutil::foreground_window().ok_or("no foreground window")?;
                if !bl.is_empty() && bl.is_blocked(&winutil::window_info(hwnd)) {
                    return Ok(Value::Null);
                }
                let mut value =
                    serde_json::to_value(winutil::to_window(hwnd)).unwrap_or(Value::Null);
                if let Some((l, t, r, b)) = winutil::visible_frame_rect(hwnd) {
                    value["visible_bounds"] = json!({
                        "x": l,
                        "y": t,
                        "width": r - l,
                        "height": b - t,
                    });
                }
                if let Some(affinity) = winutil::display_affinity(hwnd) {
                    value["display_affinity"] = json!({
                        "value": affinity,
                        "name": winutil::display_affinity_name(affinity),
                        "capture_excluded": affinity == 0x0000_0011,
                    });
                }
                Ok(value)
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Show the user-configured application blocklist (read-only). Lists which apps are redacted from screenshots/OCR and refused for control. The agent cannot change this; it is set by the user."
    )]
    async fn get_blocklist(&self) -> Result<CallToolResult, McpError> {
        let bl = &self.blocklist;
        let rules: Vec<Value> = bl
            .rules
            .iter()
            .map(|r| {
                let mode = match r.mode.unwrap_or(bl.default_mode) {
                    blocklist::RedactMode::Solid(rgb) => json!({
                        "type": "solid",
                        "color": format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]),
                    }),
                    blocklist::RedactMode::Blur(s) => json!({ "type": "blur", "sigma": s }),
                };
                json!({
                    "exe_name": r.exe_name,
                    "exe_path": r.exe_path,
                    "title": r.title,
                    "class_name": r.class_name,
                    "mode": mode,
                })
            })
            .collect();
        json_ok(&json!({
            "active": !bl.is_empty(),
            "fail_closed": bl.fail_closed,
            "rule_count": bl.rules.len(),
            "rules": rules,
            "note": "Read-only. Configured by the user via blocklist.json or OPENCONTROL_BLOCK_* \
                     environment variables; the agent cannot modify it."
        }))
    }

    #[tool(
        description = "Bring a window to the foreground and give it focus. handle from list_windows."
    )]
    async fn focus_window(
        &self,
        Parameters(p): Parameters<HandleParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let handle = p.window_handle;
        let ok = self
            .guard_hwnd(handle, move || {
                winutil::activate(winutil::hwnd_from_id(handle))
            })
            .await?;
        text_ok(format!("focus_window ok={ok}"))
    }

    #[tool(
        description = "Minimize, maximize, or restore a window. state=minimize|maximize|restore."
    )]
    async fn window_state(
        &self,
        Parameters(p): Parameters<WindowStateParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let st = p.state.clone();
        let handle = p.window_handle;
        self.guard_hwnd(handle, move || {
            winutil::window_state(winutil::hwnd_from_id(handle), &st)
        })
        .await?
        .map_err(mcp_err)?;
        text_ok(format!("window {} -> {}", p.window_handle, p.state))
    }

    #[tool(description = "Move and resize a window to an absolute desktop rectangle.")]
    async fn move_resize_window(
        &self,
        Parameters(p): Parameters<MoveResizeParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let handle = p.window_handle;
        self.guard_hwnd(handle, move || {
            winutil::set_window_bounds(winutil::hwnd_from_id(handle), p.x, p.y, p.width, p.height)
        })
        .await?
        .map_err(mcp_err)?;
        text_ok("window moved/resized")
    }

    #[tool(description = "Request a window to close (the app may prompt to save).")]
    async fn close_window(
        &self,
        Parameters(p): Parameters<HandleParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let handle = p.window_handle;
        self.guard_hwnd(handle, move || {
            winutil::close_window(winutil::hwnd_from_id(handle))
        })
        .await?
        .map_err(mcp_err)?;
        text_ok("close requested")
    }

    #[tool(description = "Launch an app by executable path or shell identifier.")]
    async fn launch_app(
        &self,
        Parameters(p): Parameters<LaunchParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = p.app.clone();
        if !self.blocklist.is_empty() {
            let info = blocklist::WindowInfo {
                exe_path: Some(app.clone()),
                ..Default::default()
            };
            if self.blocklist.is_blocked(&info) {
                return Err(blocked_err());
            }
        }
        self.worker
            .run(move || winutil::launch_app(&app))
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Launched {}.", p.app))
    }

    #[tool(
        description = "Run a PowerShell script and return stdout/stderr/exit code (output bounded)."
    )]
    async fn run_powershell(
        &self,
        Parameters(p): Parameters<PowershellParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = tokio::task::spawn_blocking(move || sys::run_powershell(&p.script, 120))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Return host system info: OS, CPU, memory, monitors, virtual screen size."
    )]
    async fn get_system_info(&self) -> Result<CallToolResult, McpError> {
        let v = tokio::task::spawn_blocking(sys::system_info)
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?;
        json_ok(&v)
    }

    #[tool(description = "Read the clipboard text.")]
    async fn get_clipboard(&self) -> Result<CallToolResult, McpError> {
        let s = tokio::task::spawn_blocking(sys::get_clipboard)
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&json!({ "text": s }))
    }

    #[tool(description = "Set the clipboard text.")]
    async fn set_clipboard(
        &self,
        Parameters(p): Parameters<SetClipboardParams>,
    ) -> Result<CallToolResult, McpError> {
        let n = p.text.len();
        tokio::task::spawn_blocking(move || sys::set_clipboard(&p.text))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        text_ok(format!("Clipboard set ({n} bytes)."))
    }

    #[tool(description = "Read a UTF-8 text file (bounded). Useful for files an app saved.")]
    async fn read_file(
        &self,
        Parameters(p): Parameters<ReadFileParams>,
    ) -> Result<CallToolResult, McpError> {
        let max = p.max_bytes.unwrap_or(200_000);
        let s = tokio::task::spawn_blocking(move || sys::read_file(&p.path, max))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        text_ok(s)
    }

    #[tool(description = "Write a UTF-8 text file. overwrite=false fails if it exists.")]
    async fn write_file(
        &self,
        Parameters(p): Parameters<WriteFileParams>,
    ) -> Result<CallToolResult, McpError> {
        let overwrite = p.overwrite.unwrap_or(false);
        tokio::task::spawn_blocking(move || sys::write_file(&p.path, &p.content, overwrite))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        text_ok("file written")
    }

    #[tool(description = "List the entries of a directory.")]
    async fn list_directory(
        &self,
        Parameters(p): Parameters<PathParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = tokio::task::spawn_blocking(move || sys::list_directory(&p.path))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(description = "Pause for a number of seconds (max 60) to let the UI settle.")]
    async fn wait(
        &self,
        Parameters(p): Parameters<WaitParams>,
    ) -> Result<CallToolResult, McpError> {
        let secs = p.seconds.clamp(0.0, 60.0);
        tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
        text_ok(format!("Waited {secs:.2}s."))
    }

    #[tool(
        description = "Primary tool: capture a window AND its indexed accessibility tree in one call. \
Returns a Set-of-Marks screenshot (numbered boxes) plus a compact tree where each control has an \
[index], center, and supported actions in {braces}. Then act by index with click_element / \
set_element_value / invoke_element. Omit window_handle to use the foreground window."
    )]
    async fn observe(
        &self,
        Parameters(p): Parameters<ObserveParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let include_text = p.include_text.unwrap_or(true);
        let marks = p.marks.unwrap_or(true);
        let detailed = matches!(
            p.response_format.as_deref(),
            Some("detailed") | Some("full")
        );
        let handle = p.window_handle;
        let max_dim = SCREENSHOT_MAX_DIM;

        let bl = self.blocklist.clone();
        let out = self
            .worker
            .run(move || -> Result<(String, String, String), String> {
                let hwnd = match handle {
                    Some(h) => winutil::hwnd_from_id(h),
                    None => winutil::foreground_window().ok_or("no foreground window")?,
                };
                if !winutil::is_window(hwnd) {
                    return Err("window is not open".into());
                }
                if !bl.is_empty() && bl.is_blocked(&winutil::window_info(hwnd)) {
                    return Err(
                        "target application is blocked by the user's OpenControl blocklist".into(),
                    );
                }
                winutil::activate(hwnd);

                let mut tree_text = String::new();
                let mut focused = String::new();
                let mut extras = String::new();
                if include_text {
                    let tr = uia::build_window_tree(hwnd, MAX_ELEMENTS, MAX_DEPTH, detailed)?;
                    tree_text = tr.tree;
                    if let Some(f) = tr.focused_element {
                        focused = f;
                    }
                    if let Some(t) = tr.selected_text {
                        extras.push_str(&format!(
                            "\nselected_text: {}",
                            &t.chars().take(200).collect::<String>()
                        ));
                    }
                    if let Some(d) = tr.document_text {
                        extras.push_str(&format!(
                            "\ndocument_text:\n{}",
                            &d.chars().take(2000).collect::<String>()
                        ));
                    }
                }

                let bm = capture::capture_window(hwnd)?;
                let img = desktop::bitmap_to_image(&bm).ok_or("failed to wrap pixels")?;
                let (mut img, scale) = desktop::scale_to(img, max_dim);
                if marks && include_text {
                    let screen_marks = uia::registry_marks();
                    let mapped: Vec<(i64, i32, i32, i32, i32)> = screen_marks
                        .iter()
                        .map(|(i, l, t, w, h)| {
                            (
                                *i,
                                ((*l - bm.origin_x) as f64 * scale).round() as i32,
                                ((*t - bm.origin_y) as f64 * scale).round() as i32,
                                (*w as f64 * scale).round() as i32,
                                (*h as f64 * scale).round() as i32,
                            )
                        })
                        .collect();
                    desktop::annotate_marks(&mut img, &mapped);
                }
                desktop::set_view(bm.origin_x, bm.origin_y, scale);
                let (b64, mime) = desktop::encode(&img, "jpeg", JPEG_QUALITY)?;

                let mut header = String::new();
                if !focused.is_empty() {
                    header.push_str(&format!("focused: {focused}\n"));
                }
                if !extras.is_empty() {
                    header.push_str(&extras);
                    header.push('\n');
                }
                if include_text {
                    let n = tree_text
                        .lines()
                        .filter(|l| l.trim_start().starts_with('['))
                        .count();
                    header.push_str(&format!(
                        "{n} elements. Act by index: click_element(element_index=N).\n{tree_text}"
                    ));
                }
                Ok((header, b64, mime))
            })
            .await
            .map_err(mcp_err)?;
        image_ok(out.0, out.1, out.2)
    }

    #[tool(
        description = "Click a control by its [index] from the latest observe (moves the real mouse to its center, or invokes it if off-screen)."
    )]
    async fn click_element(
        &self,
        Parameters(p): Parameters<ElementClickParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let idx = p.element_index;
        let button = p.button.unwrap_or_else(|| "left".into());
        let clicks = if p.double.unwrap_or(false) { 2 } else { 1 };
        let msg = self
            .worker
            .run(move || -> Result<String, String> {
                uia::focus_element(idx);
                match uia::element_center(idx) {
                    Ok((sx, sy)) => {
                        input::click(sx, sy, &button, clicks);
                        Ok(format!("Clicked element [{idx}] at ({sx},{sy})."))
                    }
                    Err(_) => {
                        uia::invoke(idx)?;
                        Ok(format!("Invoked element [{idx}] (no on-screen bounds)."))
                    }
                }
            })
            .await
            .map_err(mcp_err)?;
        text_ok(msg)
    }

    #[tool(
        description = "Set the value of an editable control by [index] via the UI Automation ValuePattern (replaces content; faster than typing)."
    )]
    async fn set_element_value(
        &self,
        Parameters(p): Parameters<SetValueParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let idx = p.element_index;
        let value = p.value.clone();
        self.worker
            .run(move || uia::set_value(idx, &value))
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Set element [{idx}]."))
    }

    #[tool(
        description = "Invoke a control's action by [index] without the mouse: auto|invoke|toggle|expand|collapse|select|'scroll up'|'scroll down'|'scroll left'|'scroll right'."
    )]
    async fn invoke_element(
        &self,
        Parameters(p): Parameters<InvokeParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let idx = p.element_index;
        let action = p.action.unwrap_or_else(|| "auto".into());
        let msg = self
            .worker
            .run(move || -> Result<String, String> {
                let act = action.to_ascii_lowercase();
                if (act == "auto" || act == "invoke") && uia::invoke(idx).is_ok() {
                    return Ok(format!("Invoked element [{idx}]."));
                }
                uia::perform_secondary(idx, &action)?;
                Ok(format!("Performed '{action}' on element [{idx}]."))
            })
            .await
            .map_err(mcp_err)?;
        text_ok(msg)
    }

    #[tool(
        description = "Read on-screen text with built-in Windows OCR (no install). Returns text plus per-word boxes in absolute screen pixels. Omit region for the whole desktop."
    )]
    async fn ocr(&self, Parameters(p): Parameters<OcrParams>) -> Result<CallToolResult, McpError> {
        guard()?;
        let region = Self::region_of(&p.region);
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                let mut bm = match region {
                    Some((x, y, w, h)) => desktop::capture_region(x, y, w, h)?,
                    None => desktop::capture_virtual()?,
                };
                redact::apply_to_bitmap(&mut bm, &bl)?;
                ocr::recognize_bitmap(bm)
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Find on-screen text via OCR and return matching words with clickable centers (absolute screen pixels)."
    )]
    async fn find_text(
        &self,
        Parameters(p): Parameters<FindTextParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let region = Self::region_of(&p.region);
        let needle = p.text.to_lowercase();
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                let mut bm = match region {
                    Some((x, y, w, h)) => desktop::capture_region(x, y, w, h)?,
                    None => desktop::capture_virtual()?,
                };
                redact::apply_to_bitmap(&mut bm, &bl)?;
                let result = ocr::recognize_bitmap(bm)?;
                let empty = vec![];
                let words = result
                    .get("words")
                    .and_then(|w| w.as_array())
                    .unwrap_or(&empty);
                let matches: Vec<Value> = words
                    .iter()
                    .filter(|w| {
                        w.get("text")
                            .and_then(|t| t.as_str())
                            .map(|t| t.to_lowercase().contains(&needle))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();
                Ok(json!({ "query": needle, "matches": matches }))
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Magnify an image-space region (x, y, width, height from the last screenshot) and return it at native resolution so small text/icons are legible. Does not change what you control."
    )]
    async fn zoom(
        &self,
        Parameters(p): Parameters<ZoomParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let (sx1, sy1) = desktop::to_screen(p.x, p.y);
        let (sx2, sy2) = desktop::to_screen(p.x + p.width, p.y + p.height);
        let (w, h) = ((sx2 - sx1).max(1), (sy2 - sy1).max(1));
        let bl = self.blocklist.clone();
        let out = self
            .worker
            .run(move || -> Result<(String, String, String), String> {
                let mut bm = desktop::capture_region(sx1, sy1, w, h)?;
                redact::apply_to_bitmap(&mut bm, &bl)?;
                let img = desktop::bitmap_to_image(&bm).ok_or("failed to wrap pixels")?;
                let (b64, mime) = desktop::encode(&img, "png", 100)?;
                let meta = json!({ "image_width": img.width(), "image_height": img.height(),
                    "note": "magnified view only; coordinate space unchanged" })
                .to_string();
                Ok((meta, b64, mime))
            })
            .await
            .map_err(mcp_err)?;
        image_ok(format!("Zoomed region.\n{}", out.0), out.1, out.2)
    }

    #[tool(description = "Double-click at image-space (x, y).")]
    async fn double_click(
        &self,
        Parameters(p): Parameters<PointParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        self.worker
            .run(move || input::click(sx, sy, "left", 2))
            .await;
        text_ok(format!("Double-clicked at image ({},{}).", p.x, p.y))
    }

    #[tool(description = "Right-click at image-space (x, y) to open a context menu.")]
    async fn right_click(
        &self,
        Parameters(p): Parameters<PointParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        self.worker
            .run(move || input::click(sx, sy, "right", 1))
            .await;
        text_ok(format!("Right-clicked at image ({},{}).", p.x, p.y))
    }

    #[tool(
        description = "Fine-grained mouse button control: action=down|up, optionally moving to image-space (x, y) first. Compose custom press-hold-release sequences."
    )]
    async fn mouse_button(
        &self,
        Parameters(p): Parameters<MouseButtonParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let button = p.button.unwrap_or_else(|| "left".into());
        let at = match (p.x, p.y) {
            (Some(x), Some(y)) => Some(desktop::to_screen(x, y)),
            _ => None,
        };
        let action = p.action.clone();
        let button_msg = button.clone();
        self.worker
            .run(move || input::mouse_button(&action, &button, at))
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Mouse {} {}.", p.action, button_msg))
    }

    #[tool(
        description = "Press a '+'-separated key chord (alias of press_key) using keysym names, e.g. 'Control_L+s'."
    )]
    async fn hotkey(
        &self,
        Parameters(p): Parameters<PressKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let keys = p.keys.clone();
        self.worker
            .run(move || input::press_chord(&keys))
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Pressed {}.", p.keys))
    }

    #[tool(
        description = "Press a sequence of key chords in order (e.g. ['alt+f','s'] for an Alt-F menu then S)."
    )]
    async fn key_combo_sequence(
        &self,
        Parameters(p): Parameters<KeySequenceParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let chords = p.chords.clone();
        self.worker
            .run(move || -> Result<(), String> {
                for c in &chords {
                    input::press_chord(c)?;
                }
                Ok(())
            })
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Pressed {} chord(s).", p.chords.len()))
    }

    #[tool(
        description = "Hold a set of keys down for a number of seconds, then release (e.g. hold shift while another action runs)."
    )]
    async fn hold_key(
        &self,
        Parameters(p): Parameters<HoldKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let keys = p.keys.clone();
        let secs = p.seconds;
        self.worker
            .run(move || input::hold_keys(&keys, secs))
            .await
            .map_err(mcp_err)?;
        text_ok(format!(
            "Held {} key(s) for {:.2}s.",
            p.keys.len(),
            p.seconds
        ))
    }

    #[tool(
        description = "List the accepted key names for press_key / hotkey / hold_key (keysym-style)."
    )]
    async fn list_key_names(&self) -> Result<CallToolResult, McpError> {
        json_ok(&json!({ "keys": opencontrol::keysym::all_names() }))
    }

    #[tool(description = "Pin a window above all others (enabled=true) or unpin it (false).")]
    async fn set_window_topmost(
        &self,
        Parameters(p): Parameters<TopmostParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let enabled = p.enabled;
        let handle = p.window_handle;
        self.guard_hwnd(handle, move || {
            winutil::set_topmost(winutil::hwnd_from_id(handle), enabled)
        })
        .await?
        .map_err(mcp_err)?;
        text_ok(format!("window {} topmost={}", p.window_handle, p.enabled))
    }

    #[tool(
        description = "Run an executable with arguments and return stdout/stderr/exit code (output bounded)."
    )]
    async fn run_command(
        &self,
        Parameters(p): Parameters<RunCommandParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = p.args.unwrap_or_default();
        let v = tokio::task::spawn_blocking(move || sys::run_command(&p.program, &args, 120))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "List running processes (pid + exe name), optionally filtered by a name substring."
    )]
    async fn list_processes(
        &self,
        Parameters(p): Parameters<ListProcessesParams>,
    ) -> Result<CallToolResult, McpError> {
        let filter = p.filter.clone();
        let limit = p.limit.unwrap_or(100);
        let v = tokio::task::spawn_blocking(move || sys::list_processes(filter.as_deref(), limit))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Terminate a process by pid, or by exe name (all matching). Destructive; use deliberately."
    )]
    async fn kill_process(
        &self,
        Parameters(p): Parameters<KillProcessParams>,
    ) -> Result<CallToolResult, McpError> {
        let name = p.name.clone();
        let pid = p.pid;
        let force = p.force.unwrap_or(false);
        let v = tokio::task::spawn_blocking(move || sys::kill_process(pid, name.as_deref(), force))
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Dump an indexed accessibility tree as text (no screenshot) for a window, or the whole desktop if window_handle is omitted. Cheaper than observe when you only need the tree."
    )]
    async fn get_ui_tree(
        &self,
        Parameters(p): Parameters<UiTreeParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let scope = p.window_handle;
        let detailed = matches!(
            p.response_format.as_deref(),
            Some("detailed") | Some("full")
        );
        // The whole-desktop tree would expose blocked windows; require a specific
        // window (still checked below) whenever a blocklist is active.
        if scope.is_none() && !self.blocklist.is_empty() {
            return Err(mcp_err(
                "a blocklist is active: pass window_handle to get_ui_tree (desktop-wide trees are \
                 disabled to avoid exposing blocked apps)",
            ));
        }
        let bl = self.blocklist.clone();
        let tree = self
            .worker
            .run(move || -> Result<String, String> {
                if let Some(h) = scope {
                    if !bl.is_empty()
                        && bl.is_blocked(&winutil::window_info(winutil::hwnd_from_id(h)))
                    {
                        return Err(
                            "target application is blocked by the user's OpenControl blocklist"
                                .into(),
                        );
                    }
                }
                uia::get_ui_tree(scope, MAX_ELEMENTS, MAX_DEPTH, detailed).map(|t| t.tree)
            })
            .await
            .map_err(mcp_err)?;
        text_ok(tree)
    }

    #[tool(
        description = "Search a window's accessibility tree for controls matching a name and/or control_type substring. Returns matches with a usable [index] for click_element."
    )]
    async fn find_elements(
        &self,
        Parameters(p): Parameters<FindElementsParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let name = p.name.clone();
        let ct = p.control_type.clone();
        let max = p.max_results.unwrap_or(30);
        let handle = p.window_handle;
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                let hwnd = winutil::hwnd_from_id(handle);
                if !bl.is_empty() && bl.is_blocked(&winutil::window_info(hwnd)) {
                    return Err(
                        "target application is blocked by the user's OpenControl blocklist".into(),
                    );
                }
                uia::find_elements(
                    hwnd,
                    name.as_deref(),
                    ct.as_deref(),
                    max,
                    MAX_ELEMENTS,
                    MAX_DEPTH,
                )
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Return the control that currently has keyboard focus (role, name, automation id, center)."
    )]
    async fn get_focused_element(&self) -> Result<CallToolResult, McpError> {
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                if !bl.is_empty() {
                    if let Some(hwnd) = winutil::foreground_window() {
                        if bl.is_blocked(&winutil::window_info(hwnd)) {
                            return Err("the focused application is blocked by the user's \
                                        OpenControl blocklist"
                                .into());
                        }
                    }
                }
                uia::focused_element()
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Identify the UI element under an image-space point (x, y) from the last screenshot."
    )]
    async fn get_element_at_point(
        &self,
        Parameters(p): Parameters<PointParams>,
    ) -> Result<CallToolResult, McpError> {
        let (sx, sy) = desktop::to_screen(p.x, p.y);
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                if !bl.is_empty() {
                    if let Some(root) = winutil::window_at_point(sx, sy) {
                        if bl.is_blocked(&winutil::window_info(root)) {
                            return Err(
                                "target application is blocked by the user's OpenControl blocklist"
                                    .into(),
                            );
                        }
                    }
                }
                uia::element_at_point(sx, sy)
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Locate a template image on screen via normalized cross-correlation. Returns matches with clickable centers (absolute screen pixels). Bound 'region' for speed."
    )]
    async fn find_image_on_screen(
        &self,
        Parameters(p): Parameters<FindImageParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let region = Self::region_of(&p.region);
        let path = p.template_path.clone();
        let threshold = p.threshold.unwrap_or(0.9);
        let all = p.all_matches.unwrap_or(false);
        let bl = self.blocklist.clone();
        let v = self
            .worker
            .run(move || -> Result<Value, String> {
                let mut bm = match region {
                    Some((x, y, w, h)) => desktop::capture_region(x, y, w, h)?,
                    None => desktop::capture_virtual()?,
                };
                redact::apply_to_bitmap(&mut bm, &bl)?;
                vision::find_image(&bm, &path, threshold, all)
            })
            .await
            .map_err(mcp_err)?;
        json_ok(&v)
    }

    #[tool(
        description = "Capture the screen (or an absolute region) and save it to a .png file on disk."
    )]
    async fn save_screenshot(
        &self,
        Parameters(p): Parameters<SaveScreenshotParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        let region = Self::region_of(&p.region);
        let path = p.path.clone();
        let bl = self.blocklist.clone();
        self.worker
            .run(move || -> Result<(), String> {
                let mut bm = match region {
                    Some((x, y, w, h)) => desktop::capture_region(x, y, w, h)?,
                    None => desktop::capture_virtual()?,
                };
                redact::apply_to_bitmap(&mut bm, &bl)?;
                desktop::save_png(&bm, &path)
            })
            .await
            .map_err(mcp_err)?;
        text_ok(format!("Saved screenshot to {}.", p.path))
    }

    #[tool(description = "Read the clipboard text.")]
    async fn paste(
        &self,
        Parameters(p): Parameters<PasteParams>,
    ) -> Result<CallToolResult, McpError> {
        guard()?;
        if let Some(text) = p.text.clone() {
            tokio::task::spawn_blocking(move || sys::set_clipboard(&text))
                .await
                .map_err(|e| mcp_err(format!("join error: {e}")))?
                .map_err(mcp_err)?;
        }
        self.worker
            .run(|| input::press_chord("ctrl+v"))
            .await
            .map_err(mcp_err)?;
        text_ok("Pasted via Ctrl+V.")
    }

    #[tool(
        description = "Copy the current selection (Ctrl+C) and return the resulting clipboard text."
    )]
    async fn copy_selection(&self) -> Result<CallToolResult, McpError> {
        guard()?;
        self.worker
            .run(|| input::press_chord("ctrl+c"))
            .await
            .map_err(mcp_err)?;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let text = tokio::task::spawn_blocking(sys::get_clipboard)
            .await
            .map_err(|e| mcp_err(format!("join error: {e}")))?
            .map_err(mcp_err)?;
        json_ok(&json!({ "text": text }))
    }

    #[tool(
        description = "Block until the screen changes from now, or until timeout. Useful after triggering an action whose completion time is unknown (e.g. a page load)."
    )]
    async fn wait_for_screen_change(
        &self,
        Parameters(p): Parameters<WaitChangeParams>,
    ) -> Result<CallToolResult, McpError> {
        let timeout = p.timeout.unwrap_or(10.0).clamp(0.5, 120.0);
        let poll = p.poll_interval.unwrap_or(0.4).max(0.1);
        let threshold = p.threshold.unwrap_or(0.01);
        let scope = p
            .scope
            .unwrap_or_else(|| "screen".to_string())
            .to_ascii_lowercase();
        let handle = p.window_handle;
        let bl = self.blocklist.clone();
        let target = self
            .worker
            .run(move || -> Result<Option<i64>, String> {
                let hwnd = match (handle, scope.as_str()) {
                    (Some(h), _) => Some(winutil::hwnd_from_id(h)),
                    (None, "foreground" | "window") => winutil::foreground_window(),
                    _ => None,
                };
                if let Some(hwnd) = hwnd {
                    if !winutil::is_window(hwnd) {
                        return Err("target window is not open".into());
                    }
                    if !bl.is_empty() && bl.is_blocked(&winutil::window_info(hwnd)) {
                        return Err(
                            "target application is blocked by the user's OpenControl blocklist"
                                .into(),
                        );
                    }
                    Ok(Some(winutil::id_from_hwnd(hwnd)))
                } else {
                    Ok(None)
                }
            })
            .await
            .map_err(mcp_err)?;

        let baseline = self
            .worker
            .run(move || {
                if let Some(h) = target {
                    capture::capture_window(winutil::hwnd_from_id(h))
                        .map(|bm| desktop::signature(&bm))
                } else {
                    desktop::capture_monitor(0).map(|bm| desktop::signature(&bm))
                }
            })
            .await
            .map_err(mcp_err)?;
        let start = std::time::Instant::now();
        let mut changed = false;
        while start.elapsed().as_secs_f64() < timeout {
            tokio::time::sleep(std::time::Duration::from_secs_f64(poll)).await;
            let cur = self
                .worker
                .run(move || {
                    if let Some(h) = target {
                        capture::capture_window(winutil::hwnd_from_id(h))
                            .map(|bm| desktop::signature(&bm))
                    } else {
                        desktop::capture_monitor(0).map(|bm| desktop::signature(&bm))
                    }
                })
                .await
                .map_err(mcp_err)?;
            if desktop::signature_diff(&baseline, &cur) >= threshold {
                changed = true;
                break;
            }
        }
        json_ok(&json!({
            "changed": changed,
            "elapsed_seconds": (start.elapsed().as_secs_f64() * 100.0).round() / 100.0,
            "scope": if target.is_some() { "window" } else { "screen" }
        }))
    }
}

#[tool_handler]
impl ServerHandler for Cu {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::V_2024_11_05;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::from_build_env();
        info.instructions = Some(INSTRUCTIONS.into());
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    set_console_title("OpenControl");
    interrupt::spawn_escape_watcher();
    let cu = Cu::new();
    let rule_count = cu.blocklist.rules.len();
    if rule_count > 0 {
        eprintln!(
            "opencontrol: {rule_count} blocklist rule(s) active \
             (matching apps are redacted from captures and refused for control)"
        );
    }
    let service = cu.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Set the console window title (no-op if not attached to a console).
fn set_console_title(title: &str) {
    use windows::core::HSTRING;
    use windows::Win32::System::Console::SetConsoleTitleW;
    let _ = unsafe { SetConsoleTitleW(&HSTRING::from(title)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse<T: for<'de> Deserialize<'de>>(v: Value) -> T {
        serde_json::from_value(v).expect("deserialize")
    }

    #[test]
    fn point_accepts_plain_integers() {
        let p: PointParams = parse(json!({ "x": 106, "y": 745 }));
        assert_eq!((p.x, p.y), (106, 745));
    }

    #[test]
    fn point_accepts_numeric_strings() {
        let p: PointParams = parse(json!({ "x": "106", "y": "745" }));
        assert_eq!((p.x, p.y), (106, 745));
    }

    #[test]
    fn point_accepts_floats() {
        let p: PointParams = parse(json!({ "x": 106.4, "y": 745.9 }));
        assert_eq!((p.x, p.y), (106, 746));
    }

    #[test]
    fn point_splits_combined_coordinate_string() {
        // The exact failure from the field report: "106, 745" in one field.
        let p: PointParams = parse(json!({ "x": "106, 745" }));
        assert_eq!((p.x, p.y), (106, 745));
    }

    #[test]
    fn point_splits_combined_array() {
        let p: PointParams = parse(json!({ "x": [106, 745] }));
        assert_eq!((p.x, p.y), (106, 745));
    }

    #[test]
    fn click_splits_combined_and_coerces_clicks() {
        let p: ClickParams = parse(json!({ "x": "100, 200", "button": "right", "clicks": "2" }));
        assert_eq!((p.x, p.y), (100, 200));
        assert_eq!(p.button.as_deref(), Some("right"));
        assert_eq!(p.clicks, Some(2));
    }

    #[test]
    fn drag_splits_four_packed_coordinates() {
        let p: DragParams = parse(json!({ "start_x": "1, 2, 3, 4" }));
        assert_eq!((p.start_x, p.start_y, p.end_x, p.end_y), (1, 2, 3, 4));
    }

    #[test]
    fn drag_accepts_separate_fields() {
        let p: DragParams = parse(json!({ "start_x": 1, "start_y": 2, "end_x": 3, "end_y": 4 }));
        assert_eq!((p.start_x, p.start_y, p.end_x, p.end_y), (1, 2, 3, 4));
    }

    #[test]
    fn zoom_splits_packed_region() {
        let p: ZoomParams = parse(json!({ "x": "10, 20, 30, 40" }));
        assert_eq!((p.x, p.y, p.width, p.height), (10, 20, 30, 40));
    }

    #[test]
    fn scroll_negative_and_string_notches() {
        let p: ScrollParams = parse(json!({ "x": 5, "y": 6, "scroll_y": "-3" }));
        assert_eq!((p.x, p.y), (5, 6));
        assert_eq!(p.scroll_y, Some(-3));
    }

    #[test]
    fn move_resize_handle_and_packed_rect() {
        let p: MoveResizeParams = parse(json!({ "window_handle": "12345", "x": "0, 0, 800, 600" }));
        assert_eq!(p.window_handle, 12345);
        assert_eq!((p.x, p.y, p.width, p.height), (0, 0, 800, 600));
    }

    #[test]
    fn mouse_button_optional_point_split() {
        let p: MouseButtonParams = parse(json!({ "action": "down", "x": "30, 40" }));
        assert_eq!(p.action, "down");
        assert_eq!((p.x, p.y), (Some(30), Some(40)));

        let none: MouseButtonParams = parse(json!({ "action": "up" }));
        assert_eq!((none.x, none.y), (None, None));
    }

    #[test]
    fn screenshot_region_from_string_and_handle_coercion() {
        let p: ScreenshotParams = parse(json!({ "region": "0, 0, 800, 600", "monitor": "2" }));
        assert_eq!(p.region, Some(vec![0, 0, 800, 600]));
        assert_eq!(p.monitor, Some(2));
    }

    #[test]
    fn screenshot_region_from_mixed_array() {
        let p: ScreenshotParams = parse(json!({ "region": ["0", 0, "800", 600] }));
        assert_eq!(p.region, Some(vec![0, 0, 800, 600]));
    }

    #[test]
    fn element_index_from_string() {
        let p: ElementClickParams = parse(json!({ "element_index": "7" }));
        assert_eq!(p.element_index, 7);
    }

    #[test]
    fn handle_from_string() {
        let p: HandleParams = parse(json!({ "window_handle": "98765" }));
        assert_eq!(p.window_handle, 98765);
    }

    #[test]
    fn wait_seconds_from_string() {
        let p: WaitParams = parse(json!({ "seconds": "1.5" }));
        assert!((p.seconds - 1.5).abs() < 1e-9);
    }
}
