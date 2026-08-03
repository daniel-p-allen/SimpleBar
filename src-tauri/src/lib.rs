//! SimpleBar — M3.
//!
//! Reads `usage.json` at startup on request from the frontend, then watches it
//! and pushes each new reading to the webview. The window owns no usage logic
//! beyond that: the Rust side yields plain data, the webview draws it.

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use tauri::{AppHandle, Emitter};

/// Event carrying a fresh reading to the webview.
const USAGE_CHANGED: &str = "usage-changed";

#[derive(Serialize, Deserialize, Clone)]
struct Usage {
    used_percentage: f64,
    resets_at: i64,
    written_at: i64,
    // #[serde(default)]: a usage.json written before this field existed
    // (by an older producer binary) must still parse, not fail closed.
    #[serde(default)]
    model_name: Option<String>,
}

/// `$XDG_STATE_HOME`, defaulting to `~/.local/state`.
///
/// Mirrors the same resolution the producer (`statusline/src/main.rs`) uses,
/// so the two agree on where the file lives without either depending on the
/// other.
fn state_dir() -> Option<PathBuf> {
    if let Ok(xdg) = env::var("XDG_STATE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    env::var("HOME").ok().map(|home| PathBuf::from(home).join(".local/state"))
}

/// The directory the producer writes into.
fn usage_dir() -> Option<PathBuf> {
    state_dir().map(|d| d.join("simplebar"))
}

/// Reads the reading the producer wrote. `Err` covers both "no data yet"
/// and "the file is unreadable" — the frontend treats both as no-data for
/// now; M7 is where those get told apart.
#[tauri::command]
fn read_usage() -> Result<Usage, String> {
    let dir = usage_dir().ok_or("no home directory")?;
    let contents = fs::read_to_string(dir.join("usage.json")).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

/// Watches `usage.json` and emits [`USAGE_CHANGED`] with each new reading.
///
/// Watches the *directory*, not the file. The producer writes atomically —
/// temp file plus rename — so every write replaces the inode. A watch on the
/// file itself would follow the old inode and go deaf after the first write.
///
/// Failure here is silent and terminal for the watch: the window keeps showing
/// the startup reading rather than dying. Live updates are an enhancement over
/// M2's snapshot, not a precondition for the window being useful.
fn spawn_watcher(app: AppHandle) {
    thread::spawn(move || {
        let Some(dir) = usage_dir() else { return };
        // The producer may not have run yet. Creating the directory up front
        // means the watch survives the first write instead of failing to
        // start, which is the ordinary case on a fresh install.
        if fs::create_dir_all(&dir).is_err() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        let Ok(mut watcher) = RecommendedWatcher::new(
            move |res| {
                // A send error means the receive loop is gone, i.e. we are
                // shutting down. Nothing useful to do about it.
                let _ = tx.send(res);
            },
            notify::Config::default(),
        ) else {
            return;
        };
        if watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
            return;
        }

        // Blocking receive, so the thread costs nothing while idle. `watcher`
        // stays alive in this scope for exactly as long as the loop runs —
        // dropping it would cancel the watch.
        for res in rx {
            let Ok(event) = res else { continue };
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                continue;
            }
            // The rename lands as an event on the directory, so check which
            // file it touched — other files in the directory are not ours.
            if !event
                .paths
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == "usage.json"))
            {
                continue;
            }
            // A half-written or malformed file is skipped rather than shown.
            // The next good write supersedes it, and the last good reading
            // stays on screen meanwhile.
            if let Ok(usage) = read_usage() {
                let _ = app.emit(USAGE_CHANGED, usage);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            spawn_watcher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![read_usage])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
