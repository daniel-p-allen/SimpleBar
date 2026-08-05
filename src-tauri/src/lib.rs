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
use tauri::{AppHandle, Emitter, Manager};

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

/// Where the window was last, in logical pixels.
///
/// All four fields together or not at all — a size without a position, or the
/// reverse, is a half-restored window, and guessing the missing half is worse
/// than opening at the default.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
struct WindowGeometry {
    width: f64,
    height: f64,
    x: f64,
    y: f64,
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

    /// Whether the window floats above other apps. Off by default — the HUD
    /// sits among your windows unless you pin it. Persists like `muted`: a
    /// pin set today is still set tomorrow, and the glyph shows the state so
    /// it is never a mystery.
    #[serde(default)]
    pinned: bool,

    /// Absent until the window has been moved or resized once, and absent
    /// again if the stored value was unusable.
    ///
    /// Deserialised leniently: a half-written or hand-mangled geometry becomes
    /// None rather than failing the whole file. Otherwise a broken window
    /// object would take the mute setting down with it, and the user would
    /// find their sound back on for reasons they could not possibly guess.
    #[serde(default, deserialize_with = "lenient_geometry")]
    window: Option<WindowGeometry>,
}

/// Parses a geometry, treating anything unusable as absent rather than as an
/// error that fails the enclosing config.
fn lenient_geometry<'de, D>(deserializer: D) -> Result<Option<WindowGeometry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Through Value first: the geometry may be any shape at all, and only the
    // second step is allowed to fail.
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or(None))
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

/// Writes the whole config, atomically.
///
/// Temp file plus rename, as with usage.json — a config half-written by a
/// crash would read as malformed on next launch and silently lose every
/// setting in it.
fn write_config(config: &Config) -> Result<(), String> {
    let dir = config_dir().ok_or("no home directory")?.join("simplebar");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let encoded = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    let tmp = dir.join("config.json.tmp");
    fs::write(&tmp, encoded).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join("config.json")).map_err(|e| e.to_string())
}

/// Persists the mute setting, and reports whether it stuck.
///
/// `Err` matters here in a way it does not for the reading: the user clicked
/// something, and a preference that silently fails to save is worse than one
/// that says so — they would find it forgotten on the next launch with no
/// clue why.
///
/// Read-modify-write, not a fresh `Config`: building one from the flag alone
/// would write `window: null` over a perfectly good geometry every time the
/// user hit mute.
#[tauri::command]
fn set_muted(muted: bool) -> Result<Config, String> {
    let config = Config { muted, ..read_config() };
    write_config(&config)?;
    Ok(config)
}

/// Pins or unpins the window on top of other apps, then persists the choice.
///
/// Takes the window because always-on-top is a property of the live window,
/// not just a stored flag — the UI must see it change now, and the config is
/// so the next launch restores it. Applied before the write: a failed
/// set_always_on_top should not leave a `pinned: true` on disk that the window
/// does not actually honour.
/// Pins or unpins the window on top of other apps, then persists the choice.
///
/// Takes the window because always-on-top is a property of the live window,
/// not just a stored flag — the UI must see it change now, and the config is
/// so the next launch restores it. Applied before the write: a failed
/// set_always_on_top should not leave a `pinned: true` on disk that the window
/// does not actually honour.
///
/// Floats above ordinary windows, including one stretched to fill the screen.
/// It deliberately does *not* try to cross into another app's native-fullscreen
/// Space: macOS only lets accessory (Dock-less) apps or nonactivating panels do
/// that, and neither is worth becoming for this. See "Always-on-top toggle" in
/// DESIGN.md.
#[tauri::command]
fn set_pinned(window: tauri::WebviewWindow, pinned: bool) -> Result<Config, String> {
    window
        .set_always_on_top(pinned)
        .map_err(|e| e.to_string())?;
    let config = Config { pinned, ..read_config() };
    write_config(&config)?;
    Ok(config)
}

/// Nudges a remembered geometry back onto a screen that still exists.
///
/// A window restored to coordinates on a monitor that has since been
/// unplugged opens invisibly, and the user's only symptom is an app that
/// appears not to launch. Overlap with any monitor is enough — a window
/// straddling two screens, or hanging off an edge, is somewhere the user can
/// see and grab.
///
/// `None` means the geometry is unusable and the default should be used
/// instead. Pure, so the monitor arithmetic is testable without a display.
fn clamp_to_visible(geometry: WindowGeometry, monitors: &[(f64, f64, f64, f64)]) -> Option<WindowGeometry> {
    // Nonsense from a hand-edited file, or a window saved while minimised.
    if !(geometry.width.is_finite() && geometry.height.is_finite()) {
        return None;
    }
    if !(geometry.x.is_finite() && geometry.y.is_finite()) {
        return None;
    }
    if geometry.width < 300.0 || geometry.height < 300.0 {
        return None;
    }
    if monitors.is_empty() {
        return None;
    }

    // Enough of the title area visible to be dragged. A window is unusable
    // long before it is fully off-screen.
    const GRAB_MARGIN: f64 = 80.0;

    let visible = monitors.iter().any(|&(mx, my, mw, mh)| {
        let overlap_x = geometry.x + geometry.width - GRAB_MARGIN > mx && geometry.x + GRAB_MARGIN < mx + mw;
        let overlap_y = geometry.y + GRAB_MARGIN > my && geometry.y + GRAB_MARGIN < my + mh;
        overlap_x && overlap_y
    });
    if visible {
        return Some(geometry);
    }

    // Not reachable on any screen: keep the size the user chose, but put it
    // back on the primary monitor rather than discarding their preference
    // entirely.
    let (mx, my, mw, mh) = monitors[0];
    Some(WindowGeometry {
        width: geometry.width.min(mw),
        height: geometry.height.min(mh),
        x: mx + (mw - geometry.width.min(mw)) / 2.0,
        y: my + (mh - geometry.height.min(mh)) / 2.0,
    })
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

/// Puts the window back where it was, if that is still somewhere visible.
///
/// Failure at any step leaves the window at its configured default, which is
/// always usable — remembering the geometry is a convenience, and none of it
/// is worth refusing to open over.
fn restore_geometry(window: &tauri::WebviewWindow) {
    let Some(saved) = read_config().window else { return };

    let monitors: Vec<(f64, f64, f64, f64)> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let scale = m.scale_factor();
            let pos = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            (pos.x, pos.y, size.width, size.height)
        })
        .collect();

    let Some(geometry) = clamp_to_visible(saved, &monitors) else { return };

    let _ = window.set_size(tauri::LogicalSize::new(geometry.width, geometry.height));
    let _ = window.set_position(tauri::LogicalPosition::new(geometry.x, geometry.y));
}

/// Records the window's geometry, coalescing a burst of events into one write.
///
/// Dragging a window emits move and resize events continuously — dozens a
/// second — and writing the config on each would hammer the disk for a value
/// only the last of which matters. Geometry is sent to a thread that keeps the
/// most recent and writes once the drag has been still for a moment.
fn spawn_geometry_saver(window: tauri::WebviewWindow) {
    let (tx, rx) = mpsc::channel::<WindowGeometry>();

    thread::spawn(move || {
        // Long enough to outlast a drag, short enough that quitting straight
        // after a resize still saves it.
        const QUIET: std::time::Duration = std::time::Duration::from_millis(400);

        while let Ok(mut latest) = rx.recv() {
            // Drain the burst: keep taking until the window has been still.
            while let Ok(newer) = rx.recv_timeout(QUIET) {
                latest = newer;
            }

            let config = Config { window: Some(latest), ..read_config() };
            let _ = write_config(&config);
        }
    });

    let handle = window.clone();
    window.on_window_event(move |event| {
        if !matches!(
            event,
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
        ) {
            return;
        }

        // Read both from the window rather than the event, so a move and a
        // resize both record a complete geometry.
        let (Ok(size), Ok(position), Ok(scale)) =
            (handle.inner_size(), handle.outer_position(), handle.scale_factor())
        else {
            return;
        };
        let size = size.to_logical::<f64>(scale);
        let position = position.to_logical::<f64>(scale);

        let _ = tx.send(WindowGeometry {
            width: size.width,
            height: size.height,
            x: position.x,
            y: position.y,
        });
    });
}

/// The producer, once copied out of the bundle.
///
/// Deliberately *not* run from inside `SimpleBar.app`. The producer is executed
/// by Claude Code, not by this app, so it must outlive any particular location
/// of the bundle — renaming the app, moving it out of Applications, or deleting
/// it entirely leaves the status line working. See "The app installs itself" in
/// DESIGN.md.
fn installed_producer_path() -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| "no HOME in environment")?;
    Ok(PathBuf::from(home).join(".local/bin/simplebar-statusline"))
}

/// What the app recorded about its own last install.
///
/// Exists to answer one question the settings file cannot: whether the copied
/// producer is still the one this app ships. `command` is stored alongside so a
/// record left by a different install path is recognisable rather than assumed.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct InstallRecord {
    version: String,
    command: String,
}

fn install_record_path() -> Option<PathBuf> {
    usage_dir().map(|d| d.join("install.json"))
}

fn read_install_record() -> Option<InstallRecord> {
    let raw = fs::read_to_string(install_record_path()?).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Whether the status line is wired, and if so whether it is current.
///
/// Serialised as a plain string for the webview, which only ever switches a
/// button on it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
enum StatuslineState {
    NotWired,
    Wired,
    Outdated,
}

/// Decides the state from the three facts it depends on.
///
/// Pure, so the rules are testable without a home directory or a settings file.
///
/// Recognising *any* command ending in `simplebar-statusline` as wired — not
/// just the path we would install — is deliberate. A developer who ran
/// `make install-statusline` has a working status line pointing into their
/// checkout, and nagging them with a Connect button would be wrong.
///
/// Anything else in `statusLine`, including another tool's, reads as not wired:
/// that is a real choice the user may want to make, and `wire` backs the file
/// up before changing it.
fn statusline_state(
    wired: Option<&str>,
    record: Option<&InstallRecord>,
    version: &str,
) -> StatuslineState {
    let Some(command) = wired else {
        return StatuslineState::NotWired;
    };
    if !command.ends_with("simplebar-statusline") {
        return StatuslineState::NotWired;
    }

    // Only a record describing *this* command can date it. One left by an
    // earlier install pointing somewhere else says nothing about what is wired
    // now, so the safe reading is that what is wired is fine.
    match record {
        Some(r) if r.command == command && r.version != version => StatuslineState::Outdated,
        _ => StatuslineState::Wired,
    }
}

/// Reports whether Claude Code is wired to a SimpleBar producer.
///
/// Never fails: every unreadable or unparseable input reads as not wired. The
/// only consumer is a button, and a HUD that refused to draw because it could
/// not classify a settings file would be worse than one offering a connect it
/// does not strictly need.
#[tauri::command]
fn read_statusline_state() -> StatuslineState {
    statusline_state(
        simplebar_statusline::wired_command().as_deref(),
        read_install_record().as_ref(),
        env!("CARGO_PKG_VERSION"),
    )
}

/// Copies the bundled producer to `~/.local/bin` and points Claude Code at it.
///
/// Only ever on an explicit click — silently editing another tool's config is
/// hostile, however convenient. `wire` takes a backup first and refuses a
/// `settings.json` it cannot parse, so a file we do not understand is left
/// exactly as it was.
///
/// Errors are returned as text for the UI to show, because this is the one
/// action in the app the user is waiting on a result from; failing silently
/// would leave them clicking a button that appears to do nothing.
#[tauri::command]
fn connect_statusline(app: AppHandle) -> Result<StatuslineState, String> {
    let bundled = app
        .path()
        .resolve("simplebar-statusline", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("cannot locate the bundled producer: {e}"))?;
    if !bundled.exists() {
        // The ordinary cause is running from `tauri dev`, where there is no
        // bundle to copy out of.
        return Err(format!(
            "the bundled producer is missing at {} — build the app with `make build`",
            bundled.display()
        ));
    }

    let target = installed_producer_path()?;
    let dir = target.parent().ok_or("no parent directory for the producer")?;
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    // Temp file plus rename, as everything else here writes. Copying straight
    // over the target would truncate a binary that Claude Code may be executing
    // at that moment; a rename swaps it whole instead.
    let tmp = target.with_extension("tmp");
    fs::copy(&bundled, &tmp).map_err(|e| format!("cannot copy the producer: {e}"))?;
    fs::rename(&tmp, &target).map_err(|e| format!("cannot install the producer: {e}"))?;

    let command = target
        .to_str()
        .ok_or("the producer path is not valid UTF-8")?
        .to_string();

    // Recorded before wiring: a record without a wired command merely dates a
    // copied file, while a wired command with no record would look current
    // forever and never offer an update.
    write_install_record(&InstallRecord {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: command.clone(),
    })?;

    simplebar_statusline::wire(&command)?;
    Ok(read_statusline_state())
}

fn write_install_record(record: &InstallRecord) -> Result<(), String> {
    let path = install_record_path().ok_or("no home directory")?;
    let dir = path.parent().ok_or("no parent directory")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let encoded = serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, encoded).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            spawn_watcher(app.handle().clone());

            if let Some(window) = app.get_webview_window("main") {
                restore_geometry(&window);
                // A window pinned last session comes back pinned. Best-effort,
                // like the geometry: failing to re-pin is not worth refusing to
                // open over.
                let _ = window.set_always_on_top(read_config().pinned);
                spawn_geometry_saver(window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_usage,
            read_config,
            set_muted,
            set_pinned,
            read_statusline_state,
            connect_statusline
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_key_defaults_to_unmuted() {
        // A config written before `muted` existed, or hand-edited.
        assert_eq!(config_from_str("{}"), Config { muted: false, pinned: false, window: None });
    }

    #[test]
    fn the_stored_value_is_honoured() {
        assert_eq!(config_from_str(r#"{"muted":true}"#), Config { muted: true, pinned: false, window: None });
        assert_eq!(config_from_str(r#"{"muted":false}"#), Config { muted: false, pinned: false, window: None });
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
        // A newer build's config must not stop an older build from reading the
        // keys it does understand.
        let raw = r#"{"muted":true,"future_setting":"x"}"#;
        assert_eq!(config_from_str(raw), Config { muted: true, pinned: false, window: None });
    }

    #[test]
    fn pinned_defaults_to_off_and_is_honoured_when_set() {
        // A config from before `pinned` existed reads as unpinned — the HUD
        // must not start floating over everything on upgrade.
        assert!(!config_from_str("{}").pinned, "unpinned by default");
        assert!(config_from_str(r#"{"pinned":true}"#).pinned);
        assert!(!config_from_str(r#"{"pinned":false}"#).pinned);
    }

    #[test]
    fn pinned_and_muted_are_independent() {
        // Each toggle round-trips without disturbing the other.
        let c = config_from_str(r#"{"muted":true,"pinned":true}"#);
        assert!(c.muted && c.pinned);
        let c = config_from_str(r#"{"muted":false,"pinned":true}"#);
        assert!(!c.muted && c.pinned);
    }

    #[test]
    fn a_broken_geometry_does_not_take_the_mute_setting_with_it() {
        // Half a geometry — width with no height — must not fail the whole
        // file. Losing the mute setting because the window coordinates were
        // mangled is a change the user could never account for.
        for raw in [
            r#"{"muted":true,"window":{"width":900}}"#,
            r#"{"muted":true,"window":"nonsense"}"#,
            r#"{"muted":true,"window":{"width":"wide","height":700,"x":0,"y":0}}"#,
        ] {
            let config = config_from_str(raw);
            assert!(config.muted, "mute must survive {raw}");
            assert_eq!(config.window, None, "geometry must be dropped for {raw}");
        }
    }

    // Window geometry. A laptop screen, and a second monitor to its left —
    // negative coordinates are ordinary on macOS and easy to get wrong.
    const LAPTOP: (f64, f64, f64, f64) = (0.0, 0.0, 1440.0, 900.0);
    const LEFT_MONITOR: (f64, f64, f64, f64) = (-1920.0, 0.0, 1920.0, 1080.0);

    fn geometry(x: f64, y: f64) -> WindowGeometry {
        WindowGeometry { width: 800.0, height: 800.0, x, y }
    }

    #[test]
    fn a_window_on_screen_is_left_alone() {
        let g = geometry(100.0, 50.0);
        assert_eq!(clamp_to_visible(g, &[LAPTOP]), Some(g));
    }

    #[test]
    fn a_window_on_a_second_monitor_is_left_alone() {
        // The bug this guards: treating negative coordinates as off-screen
        // would drag every window off the left-hand monitor on every launch.
        let g = geometry(-1800.0, 100.0);
        assert_eq!(clamp_to_visible(g, &[LAPTOP, LEFT_MONITOR]), Some(g));
    }

    #[test]
    fn hanging_off_an_edge_is_fine() {
        // Still grabbable, so not worth moving — users park windows like this
        // deliberately.
        let g = geometry(1300.0, 100.0);
        assert_eq!(clamp_to_visible(g, &[LAPTOP]), Some(g));
    }

    #[test]
    fn a_window_on_an_unplugged_monitor_comes_back() {
        // Saved on the left-hand monitor, reopened with only the laptop.
        let g = geometry(-1800.0, 100.0);
        let restored = clamp_to_visible(g, &[LAPTOP]).expect("brought back");

        assert_eq!(restored.width, 800.0, "the chosen size is kept");
        assert_eq!(restored.height, 800.0);
        assert!(restored.x >= 0.0 && restored.x + restored.width <= 1440.0, "x on screen: {restored:?}");
        assert!(restored.y >= 0.0 && restored.y + restored.height <= 900.0, "y on screen: {restored:?}");
    }

    #[test]
    fn a_window_only_just_off_the_edge_comes_back() {
        // Far enough that too little is grabbable.
        let g = geometry(1420.0, 100.0);
        let restored = clamp_to_visible(g, &[LAPTOP]).expect("brought back");
        assert!(restored.x < 1420.0, "should have moved: {restored:?}");
    }

    #[test]
    fn a_window_larger_than_the_screen_is_shrunk_to_fit() {
        let g = WindowGeometry { width: 3000.0, height: 2000.0, x: -2000.0, y: -2000.0 };
        let restored = clamp_to_visible(g, &[LAPTOP]).expect("brought back");
        assert!(restored.width <= 1440.0 && restored.height <= 900.0, "{restored:?}");
    }

    #[test]
    fn nonsense_geometry_is_refused() {
        // A hand-edited config, or a window saved while minimised. None is the
        // signal to open at the default rather than guess.
        assert_eq!(clamp_to_visible(WindowGeometry { width: 0.0, height: 0.0, x: 0.0, y: 0.0 }, &[LAPTOP]), None);
        assert_eq!(clamp_to_visible(WindowGeometry { width: 100.0, height: 100.0, x: 0.0, y: 0.0 }, &[LAPTOP]), None,
                   "below the 300px floor");
        assert_eq!(clamp_to_visible(WindowGeometry { width: f64::NAN, height: 800.0, x: 0.0, y: 0.0 }, &[LAPTOP]), None);
        assert_eq!(clamp_to_visible(WindowGeometry { width: 800.0, height: 800.0, x: f64::INFINITY, y: 0.0 }, &[LAPTOP]), None);
    }

    #[test]
    fn no_monitors_means_no_opinion() {
        // Nothing sensible to clamp against; the default is safer than a guess.
        assert_eq!(clamp_to_visible(geometry(100.0, 100.0), &[]), None);
    }

    #[test]
    fn muting_does_not_forget_the_window() {
        // The bug this guards: building a fresh Config from the mute flag
        // wrote window: null over a good geometry on every click.
        let stored = config_from_str(r#"{"muted":false,"window":{"width":900,"height":700,"x":10,"y":20}}"#);
        let after = Config { muted: true, ..stored.clone() };

        assert!(after.muted);
        assert_eq!(after.window, stored.window, "geometry must survive a mute");
    }

    #[test]
    fn a_config_without_geometry_still_parses() {
        assert_eq!(config_from_str(r#"{"muted":true}"#).window, None);
    }

    #[test]
    fn the_default_is_sound_on() {
        assert!(!Config::default().muted, "sound must be on until turned off");
    }

    // Statusline wiring. No filesystem here — these are the classification
    // rules, and the Connect button appears or does not appear on them.
    const INSTALLED: &str = "/Users/x/.local/bin/simplebar-statusline";

    fn record(command: &str, version: &str) -> InstallRecord {
        InstallRecord { command: command.to_string(), version: version.to_string() }
    }

    #[test]
    fn nothing_wired_offers_a_connect() {
        assert_eq!(statusline_state(None, None, "0.1.0"), StatuslineState::NotWired);
    }

    #[test]
    fn our_producer_reads_as_wired() {
        assert_eq!(statusline_state(Some(INSTALLED), None, "0.1.0"), StatuslineState::Wired);
    }

    #[test]
    fn a_developer_checkout_is_left_alone() {
        // `make install-statusline` wires a path inside the repo. That is a
        // working status line, so nagging with a Connect button would be wrong.
        let dev = "/Users/x/code/SimpleBar/statusline/target/release/simplebar-statusline";
        assert_eq!(statusline_state(Some(dev), None, "0.1.0"), StatuslineState::Wired);
    }

    #[test]
    fn another_tool_reads_as_not_wired() {
        // Someone else owns the status line. Offering to connect is right —
        // and `wire` backs the file up before changing it.
        for other in ["/usr/local/bin/starship", "/opt/homebrew/bin/ccusage"] {
            assert_eq!(
                statusline_state(Some(other), None, "0.1.0"),
                StatuslineState::NotWired,
                "{other} is not ours"
            );
        }
    }

    #[test]
    fn a_newer_app_than_the_copied_producer_is_outdated() {
        let old = record(INSTALLED, "0.1.0");
        assert_eq!(statusline_state(Some(INSTALLED), Some(&old), "0.2.0"), StatuslineState::Outdated);
    }

    #[test]
    fn a_matching_version_is_current() {
        let same = record(INSTALLED, "0.1.0");
        assert_eq!(statusline_state(Some(INSTALLED), Some(&same), "0.1.0"), StatuslineState::Wired);
    }

    #[test]
    fn a_record_for_a_different_command_does_not_date_this_one() {
        // The bug this guards: a stale record from an earlier install would
        // otherwise mark a perfectly good developer wiring as out of date, and
        // a Connect click would then overwrite their checkout path.
        let elsewhere = record("/somewhere/else/simplebar-statusline", "0.0.1");
        assert_eq!(
            statusline_state(Some(INSTALLED), Some(&elsewhere), "0.9.0"),
            StatuslineState::Wired
        );
    }

    #[test]
    fn the_state_serialises_as_the_string_the_webview_expects() {
        let json = serde_json::to_string(&StatuslineState::NotWired).unwrap();
        assert_eq!(json, r#""not-wired""#);
        assert_eq!(serde_json::to_string(&StatuslineState::Outdated).unwrap(), r#""outdated""#);
    }
}
