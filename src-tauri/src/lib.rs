//! SimpleBar — the window.
//!
//! Reads `usage.json` at startup on request from the frontend, then watches it
//! and pushes each new reading to the webview. The window owns no usage logic
//! beyond that: the Rust side yields plain data, the webview draws it.
//!
//! Also owns `config.json`, the one place a preference is remembered between
//! runs. The webview asks for it and asks for it to change; it never learns
//! where it lives.

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
    // #[serde(default)]: a usage.json written before these fields existed
    // (by an older producer binary) must still parse, not fail closed.
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    effort_level: Option<String>,
}

/// Preferences remembered between runs.
///
/// Every field defaults, and the whole file is optional: a first run has no
/// config.json at all, and a hand-edited one may be missing keys or malformed.
/// None of that is worth an error message on a HUD — the defaults are the
/// unsurprising behaviour, so a broken file quietly behaves like a fresh
/// install rather than blocking the window.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
struct Config {
    /// Sound is on unless the user turned it off. Muting persists until they
    /// unmute — it does not expire daily and the app never asks. A mute that
    /// silently lapses would beep when the user thought they were safe from
    /// it; what makes indefinite muting safe is that the glyph shows the
    /// state, so a mute set last week is visible rather than mysterious.
    #[serde(default)]
    muted: bool,
}

/// `$XDG_CONFIG_HOME`, defaulting to `~/.config`.
fn config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    env::var("HOME").ok().map(|home| PathBuf::from(home).join(".config"))
}

/// Parses config text, falling back to defaults on anything unusable.
///
/// Separate from the file read so the fallback behaviour is testable without
/// a filesystem.
fn config_from_str(raw: &str) -> Config {
    serde_json::from_str(raw).unwrap_or_default()
}

#[tauri::command]
fn read_config() -> Config {
    let Some(dir) = config_dir() else { return Config::default() };
    let Ok(raw) = fs::read_to_string(dir.join("simplebar").join("config.json")) else {
        // No file yet is the ordinary first-run case, not an error.
        return Config::default();
    };
    config_from_str(&raw)
}

/// Persists the config, atomically, and reports whether it stuck.
///
/// `Err` matters here in a way it does not for the reading: the user clicked
/// something, and a preference that silently fails to save is worse than one
/// that says so — they would find it forgotten on the next launch with no
/// clue why.
#[tauri::command]
fn set_muted(muted: bool) -> Result<Config, String> {
    let dir = config_dir().ok_or("no home directory")?.join("simplebar");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let config = Config { muted };
    let encoded = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;

    // Temp file plus rename, as with usage.json — a config half-written by a
    // crash would read as malformed on next launch and silently lose the
    // setting.
    let tmp = dir.join("config.json.tmp");
    fs::write(&tmp, encoded).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join("config.json")).map_err(|e| e.to_string())?;

    Ok(config)
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
        .invoke_handler(tauri::generate_handler![read_usage, read_config, set_muted])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_key_defaults_to_unmuted() {
        // A config written before `muted` existed, or hand-edited.
        assert_eq!(config_from_str("{}"), Config { muted: false });
    }

    #[test]
    fn the_stored_value_is_honoured() {
        assert_eq!(config_from_str(r#"{"muted":true}"#), Config { muted: true });
        assert_eq!(config_from_str(r#"{"muted":false}"#), Config { muted: false });
    }

    #[test]
    fn unusable_config_falls_back_to_defaults() {
        // Sound on is the unsurprising behaviour, and a HUD is the wrong place
        // to report a broken preferences file. Each of these must behave like
        // a fresh install rather than blocking the window.
        for raw in ["", "{ not json", "null", "[]", "42", r#"{"muted":"yes"}"#] {
            assert_eq!(config_from_str(raw), Config::default(), "{raw:?} should default");
        }
    }

    #[test]
    fn unknown_keys_are_ignored_not_fatal() {
        // A newer build's config, or window geometry added later, must not
        // stop an older build from reading the mute flag it does understand.
        let raw = r#"{"muted":true,"window":{"width":900},"future_setting":"x"}"#;
        assert_eq!(config_from_str(raw), Config { muted: true });
    }

    #[test]
    fn the_default_is_sound_on() {
        assert!(!Config::default().muted, "sound must be on until turned off");
    }
}
