use serde_json::{json, Value};
use std::process::Command;

pub fn system_info() -> Value {
    use windows::Win32::System::SystemInformation::{
        GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CMONITORS, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };

    let mut si = SYSTEM_INFO::default();
    unsafe { GetSystemInfo(&mut si) };

    let mut mem = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let (total_mb, avail_mb) = unsafe {
        if GlobalMemoryStatusEx(&mut mem).is_ok() {
            (
                (mem.ullTotalPhys / 1024 / 1024) as i64,
                (mem.ullAvailPhys / 1024 / 1024) as i64,
            )
        } else {
            (0, 0)
        }
    };

    let (vw, vh, monitors) = unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            GetSystemMetrics(SM_CMONITORS),
        )
    };

    json!({
        "os": "Windows",
        "arch": std::env::consts::ARCH,
        "cpu_logical": si.dwNumberOfProcessors,
        "memory_total_mb": total_mb,
        "memory_available_mb": avail_mb,
        "monitors": monitors,
        "virtual_screen": { "width": vw, "height": vh },
        "hostname": hostname(),
    })
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
}

// ---------------------------------------------------------------------------
// Clipboard (text)
// ---------------------------------------------------------------------------
pub fn get_clipboard() -> Result<String, String> {
    use clipboard_win::{formats, get_clipboard};
    get_clipboard(formats::Unicode).map_err(|e| format!("clipboard read failed: {e}"))
}

pub fn set_clipboard(text: &str) -> Result<(), String> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|e| format!("clipboard write failed: {e}"))
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------
/// Run a PowerShell script and capture stdout/stderr/exit code (bounded output).
pub fn run_powershell(script: &str, timeout_secs: u64) -> Result<Value, String> {
    run_capture(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
        timeout_secs,
    )
}

/// Run an arbitrary command with args and capture output.
pub fn run_command(program: &str, args: &[String], timeout_secs: u64) -> Result<Value, String> {
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_capture(program, &refs, timeout_secs)
}

fn run_capture(program: &str, args: &[&str], _timeout_secs: u64) -> Result<Value, String> {
    // Note: std has no built-in timeout; the MCP layer caps via its own deadline.
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("failed to launch {program}: {e}"))?;
    let cap = 60_000usize;
    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if stdout.len() > cap {
        stdout.truncate(cap);
        stdout.push_str("\n...[truncated]");
    }
    if stderr.len() > cap {
        stderr.truncate(cap);
        stderr.push_str("\n...[truncated]");
    }
    Ok(json!({
        "exit_code": output.status.code(),
        "stdout": stdout,
        "stderr": stderr,
    }))
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------
pub fn read_file(path: &str, max_bytes: usize) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let slice = if data.len() > max_bytes {
        &data[..max_bytes]
    } else {
        &data[..]
    };
    Ok(String::from_utf8_lossy(slice).to_string())
}

pub fn write_file(path: &str, content: &str, overwrite: bool) -> Result<(), String> {
    if !overwrite && std::path::Path::new(path).exists() {
        return Err(format!("{path} already exists (set overwrite=true)"));
    }
    std::fs::write(path, content).map_err(|e| format!("write failed: {e}"))
}

pub fn list_directory(path: &str) -> Result<Value, String> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| format!("list failed: {e}"))? {
        let entry = entry.map_err(|e| format!("entry error: {e}"))?;
        let meta = entry.metadata().ok();
        entries.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "is_dir": meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
            "size": meta.as_ref().map(|m| m.len()).unwrap_or(0),
        }));
    }
    Ok(json!({ "path": path, "entries": entries }))
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------
fn snapshot_processes() -> Result<Vec<(u32, String)>, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| format!("snapshot failed: {e}"))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                out.push((entry.th32ProcessID, name));
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    Ok(out)
}

/// List running processes (pid + exe name), optionally filtered by a name
/// substring, capped at `limit`.
pub fn list_processes(filter: Option<&str>, limit: usize) -> Result<Value, String> {
    let needle = filter.map(|f| f.to_lowercase());
    let mut procs: Vec<Value> = snapshot_processes()?
        .into_iter()
        .filter(|(_, name)| {
            needle
                .as_ref()
                .map(|n| name.to_lowercase().contains(n))
                .unwrap_or(true)
        })
        .map(|(pid, name)| json!({ "pid": pid, "name": name }))
        .collect();
    let total = procs.len();
    procs.truncate(limit.max(1));
    Ok(json!({ "count": total, "returned": procs.len(), "processes": procs }))
}

/// Terminate a process by pid, or by exe name (all matching). Returns killed pids.
pub fn kill_process(pid: Option<u32>, name: Option<&str>, force: bool) -> Result<Value, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let _ = force; // TerminateProcess is already forceful; flag kept for parity.

    let targets: Vec<u32> = if let Some(p) = pid {
        vec![p]
    } else if let Some(n) = name {
        let nl = n.to_lowercase();
        snapshot_processes()?
            .into_iter()
            .filter(|(_, pname)| {
                pname.to_lowercase() == nl || pname.to_lowercase() == format!("{nl}.exe")
            })
            .map(|(p, _)| p)
            .collect()
    } else {
        return Err("provide pid or name".into());
    };
    if targets.is_empty() {
        return Err("no matching process".into());
    }

    let mut killed = Vec::new();
    let mut errors = Vec::new();
    for p in targets {
        unsafe {
            match OpenProcess(PROCESS_TERMINATE, false, p) {
                Ok(h) => {
                    if TerminateProcess(h, 1).is_ok() {
                        killed.push(p);
                    } else {
                        errors.push(format!("pid {p}: terminate failed"));
                    }
                    let _ = CloseHandle(h);
                }
                Err(e) => errors.push(format!("pid {p}: {e}")),
            }
        }
    }
    Ok(json!({ "killed": killed, "errors": errors }))
}
