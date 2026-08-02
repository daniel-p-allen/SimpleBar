//! SimpleBar — M2.
//!
//! Reads `usage.json` once at startup, on request from the frontend, and
//! hands the reading back as plain data. No file watching yet (M3), no SVG
//! yet (M4) — this milestone exists only to prove the number the producer
//! wrote actually reaches the window.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
struct Usage {
    used_percentage: f64,
    resets_at: i64,
    written_at: i64,
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

/// Reads the reading the producer wrote. `Err` covers both "no data yet"
/// and "the file is unreadable" — the frontend treats both as no-data for
/// now; M7 is where those get told apart.
#[tauri::command]
fn read_usage() -> Result<Usage, String> {
    let dir = state_dir().ok_or("no home directory")?;
    let path = dir.join("simplebar").join("usage.json");
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![read_usage])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
