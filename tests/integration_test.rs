use std::io::Write;
use std::process::{Command, Stdio};

/// Integration test: ensure the server starts and responds to MCP initialize
#[test]
fn test_mcp_server_initialize() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_OpenControl"));
    if !exe.exists() {
        eprintln!("Server exe not built. Run 'cargo build --release' first.");
        return;
    }

    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let mut stdout = child.stdout.take().expect("Failed to open stdout");

    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test",
                "version": "1.0"
            }
        }
    });

    stdin
        .write_all(format!("{init_request}\n").as_bytes())
        .expect("Failed to write to stdin");

    let mut response = String::new();
    let mut buf = [0u8; 1024];

    // Read first response (initialize)
    if let Ok(n) = std::io::Read::read(&mut stdout, &mut buf) {
        response = String::from_utf8_lossy(&buf[..n]).to_string();
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(!response.is_empty(), "No response from server");
    assert!(
        response.contains("\"id\":1"),
        "Response missing expected request ID"
    );
    assert!(
        response.contains("serverInfo"),
        "Response missing serverInfo"
    );
}

/// Integration test: check that tools/list returns expected tools
#[test]
fn test_mcp_tools_list() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_OpenControl"));
    if !exe.exists() {
        eprintln!("Server exe not built. Run 'cargo build --release' first.");
        return;
    }

    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");

    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }),
    ];

    for req in &requests {
        stdin
            .write_all(format!("{req}\n").as_bytes())
            .expect("Failed to write request");
    }

    let mut response = String::new();
    let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
    for _ in 0..10 {
        let mut line = String::new();
        if std::io::BufRead::read_line(&mut reader, &mut line).unwrap_or(0) == 0 {
            break;
        }
        response.push_str(&line);
        if line.contains("\"id\":2") {
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(response.contains("tools"), "Response missing tools");
    assert!(
        response.contains("screen_info"),
        "Tools list missing screen_info"
    );
    assert!(
        response.contains("take_screenshot"),
        "Tools list missing take_screenshot"
    );
}

// ---------------------------------------------------------------------------
// Blocklist / screenshot-redaction end-to-end test.
//
// Uses two server instances against a real Notepad window: an unblocked one to
// discover Notepad's handle, and one started with OPENCONTROL_BLOCK_EXE set, to
// verify the blocked app is filtered, refused, and still captures cleanly.
// Skips gracefully (does not fail) when no Notepad window appears, e.g. on a
// headless/locked-down CI desktop.
// ---------------------------------------------------------------------------

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, ChildStdout};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

struct Server {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    id: i64,
}

impl Server {
    fn start(envs: &[(&str, &str)]) -> Server {
        let exe = std::path::Path::new(env!("CARGO_BIN_EXE_OpenControl"));
        let mut cmd = Command::new(exe);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("spawn server");
        let stdin = child.stdin.take().expect("stdin");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        Server {
            child,
            stdin,
            reader,
            id: 0,
        }
    }

    fn send(&mut self, v: &Value) {
        self.stdin
            .write_all(format!("{v}\n").as_bytes())
            .expect("write");
        self.stdin.flush().expect("flush");
    }

    /// Read JSON lines until one carries the wanted id (skips notifications).
    fn read_id(&mut self, want: i64) -> Option<Value> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let mut line = String::new();
            if self.reader.read_line(&mut line).unwrap_or(0) == 0 {
                return None;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if v.get("id").and_then(|i| i.as_i64()) == Some(want) {
                    return Some(v);
                }
            }
        }
        None
    }

    fn initialize(&mut self) {
        self.id += 1;
        let id = self.id;
        self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "itest", "version": "0"}
            }
        }));
        self.read_id(id).expect("initialize response");
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    /// Call a tool. Returns the `result` object on success, or `Err(message)`
    /// when the server returns a JSON-RPC error.
    fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        self.id += 1;
        let id = self.id;
        self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}
        }));
        let msg = self.read_id(id).ok_or_else(|| "no response".to_string())?;
        if let Some(err) = msg.get("error") {
            return Err(err.to_string());
        }
        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Concatenate the text content blocks of a tool result.
fn result_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(|c| c.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn result_has_image(result: &Value) -> bool {
    result
        .get("content")
        .and_then(|c| c.as_array())
        .map(|items| {
            items
                .iter()
                .any(|c| c.get("type").and_then(|t| t.as_str()) == Some("image"))
        })
        .unwrap_or(false)
}

/// Find a Notepad window in a `list_windows` result; returns (handle, app, title).
fn find_notepad(list_windows: &Value) -> Option<(i64, String)> {
    let text = result_text(list_windows);
    let v: Value = serde_json::from_str(&text).ok()?;
    for w in v.get("windows")?.as_array()? {
        let app = w.get("app").and_then(|a| a.as_str()).unwrap_or("");
        if app.to_lowercase().ends_with("notepad.exe") {
            let id = w.get("id").and_then(|i| i.as_i64())?;
            return Some((id, app.to_string()));
        }
    }
    None
}

#[test]
fn test_blocklist_redaction_and_refusal() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_OpenControl"));
    if !exe.exists() {
        eprintln!("Server exe not built; skipping. Run 'cargo build --release' first.");
        return;
    }

    // Launch Notepad (the modern launcher may exit; the real window is owned by
    // a separate process, so we close it via the server later).
    let mut notepad = match Command::new("notepad.exe").spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("could not launch notepad ({e}); skipping blocklist test");
            return;
        }
    };

    // Discover Notepad's handle through an unblocked server.
    let mut unblocked = Server::start(&[]);
    unblocked.initialize();

    let mut found = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(lw) = unblocked.call("list_windows", json!({})) {
            if let Some(hit) = find_notepad(&lw) {
                found = Some(hit);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    let (handle, app) = match found {
        Some(v) => v,
        None => {
            eprintln!("no Notepad window appeared (headless desktop?); skipping blocklist test");
            let _ = notepad.kill();
            let _ = notepad.wait();
            return;
        }
    };
    eprintln!("found Notepad handle={handle} app={app}");

    // Start a server with Notepad blocked.
    let mut blocked = Server::start(&[("OPENCONTROL_BLOCK_EXE", "notepad.exe")]);
    blocked.initialize();

    // get_blocklist reports the active rule.
    let bl = blocked
        .call("get_blocklist", json!({}))
        .expect("get_blocklist");
    let bl_json: Value = serde_json::from_str(&result_text(&bl)).expect("blocklist json");
    assert_eq!(bl_json["active"], json!(true), "blocklist should be active");
    assert!(
        bl_json["rule_count"].as_i64().unwrap_or(0) >= 1,
        "expected at least one blocklist rule"
    );

    // list_windows must omit the blocked app.
    let lw = blocked
        .call("list_windows", json!({}))
        .expect("list_windows");
    assert!(
        find_notepad(&lw).is_none(),
        "blocked Notepad must not appear in list_windows: {}",
        result_text(&lw)
    );

    // focus_window on the blocked handle is refused.
    let focus = blocked.call("focus_window", json!({ "window_handle": handle }));
    assert!(
        focus.is_err(),
        "focus_window on a blocked window should error"
    );
    assert!(
        focus.unwrap_err().to_lowercase().contains("block"),
        "error should mention the blocklist"
    );

    // observe on the blocked handle is refused.
    let obs = blocked.call("observe", json!({ "window_handle": handle }));
    assert!(obs.is_err(), "observe on a blocked window should error");

    // take_screenshot still succeeds (the blocked window is redacted, not refused).
    let ss = blocked
        .call("take_screenshot", json!({ "max_dimension": 800 }))
        .expect("take_screenshot should still return an image");
    assert!(result_has_image(&ss), "screenshot should contain an image");

    // Clean up: close Notepad via the unblocked server, then drop everything.
    let _ = unblocked.call("close_window", json!({ "window_handle": handle }));
    drop(blocked);
    drop(unblocked);
    let _ = notepad.kill();
    let _ = notepad.wait();
}
