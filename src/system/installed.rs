use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows::Win32::System::Com::{
    CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
};
use windows::Win32::System::ProcessStatus::{EnumProcesses, GetModuleFileNameExW};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

use crate::protocol::{AppEntry, Window};
use crate::winutil;

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(pd) = std::env::var("ProgramData") {
        dirs.push(PathBuf::from(pd).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(ad) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(ad).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    dirs
}

fn collect_lnks(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 || out.len() > 2000 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lnks(&path, out, depth + 1);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Resolve a `.lnk` shortcut to its target executable path.
fn resolve_lnk(lnk: &Path) -> Option<String> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let wide: Vec<u16> = lnk.as_os_str().encode_wide_nul();
        pf.Load(PCWSTR(wide.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut fd = WIN32_FIND_DATAW::default();
        link.GetPath(&mut buf, &mut fd, 0).ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        if end == 0 {
            return None;
        }
        let target = String::from_utf16_lossy(&buf[..end]);
        if target.to_ascii_lowercase().ends_with(".exe") {
            Some(target)
        } else {
            None
        }
    }
}

trait EncodeWideNul {
    fn encode_wide_nul(&self) -> Vec<u16>;
}
impl EncodeWideNul for std::ffi::OsStr {
    fn encode_wide_nul(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain(std::iter::once(0)).collect()
    }
}

/// Full executable paths (lowercased) of currently running processes.
fn running_exe_paths() -> Vec<String> {
    let mut pids = vec![0u32; 2048];
    let mut needed = 0u32;
    let out = unsafe {
        if EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut needed,
        )
        .is_err()
        {
            return Vec::new();
        }
        let count = needed as usize / std::mem::size_of::<u32>();
        let mut paths = Vec::new();
        for &pid in pids.iter().take(count) {
            if pid == 0 {
                continue;
            }
            if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                let mut buf = [0u16; MAX_PATH as usize];
                let n = GetModuleFileNameExW(h, None, &mut buf);
                let _ = CloseHandle(h);
                if n > 0 {
                    paths.push(String::from_utf16_lossy(&buf[..n as usize]).to_ascii_lowercase());
                }
            }
        }
        paths
    };
    out
}

fn display_name(target: &str, lnk: &Path) -> String {
    // Prefer the shortcut's name (user-facing) over the exe stem.
    lnk.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            Path::new(target)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(target)
                .to_string()
        })
}

/// Enumerate installed apps, marking running state and attaching open windows.
pub fn list_installed_apps() -> Vec<AppEntry> {
    // Open windows grouped by owning exe path (lowercased key -> windows).
    let mut windows_by_exe: BTreeMap<String, Vec<Window>> = BTreeMap::new();
    for w in winutil::list_windows() {
        windows_by_exe
            .entry(w.app.to_ascii_lowercase())
            .or_default()
            .push(w);
    }

    let running: Vec<String> = running_exe_paths();
    let is_running = |target_lc: &str| -> bool {
        running.iter().any(|p| p == target_lc) || windows_by_exe.contains_key(target_lc)
    };

    // Resolve installed shortcuts -> target exe, deduped by target path.
    let mut lnks = Vec::new();
    for dir in start_menu_dirs() {
        collect_lnks(&dir, &mut lnks, 0);
    }

    let mut apps: BTreeMap<String, AppEntry> = BTreeMap::new();
    for lnk in &lnks {
        if let Some(target) = resolve_lnk(lnk) {
            let key = target.to_ascii_lowercase();
            if apps.contains_key(&key) {
                continue;
            }
            let windows = windows_by_exe.get(&key).cloned().unwrap_or_default();
            apps.insert(
                key.clone(),
                AppEntry {
                    id: target.clone(),
                    display_name: Some(display_name(&target, lnk)),
                    is_running: Some(is_running(&key)),
                    last_used_date: None,
                    use_count: None,
                    windows,
                },
            );
        }
    }

    // Fold in running apps that have open windows but no Start Menu shortcut, so
    // every targetable window is still represented.
    for (key, wins) in &windows_by_exe {
        if apps.contains_key(key) {
            continue;
        }
        let display = Path::new(key)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        apps.insert(
            key.clone(),
            AppEntry {
                id: wins
                    .first()
                    .map(|w| w.app.clone())
                    .unwrap_or_else(|| key.clone()),
                display_name: display,
                is_running: Some(true),
                last_used_date: None,
                use_count: None,
                windows: wins.clone(),
            },
        );
    }

    apps.into_values().collect()
}

#[allow(dead_code)]
fn _unused(_p: *mut c_void) {}
