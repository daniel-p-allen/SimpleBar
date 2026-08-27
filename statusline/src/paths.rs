//! Where SimpleBar's files live, on every platform it runs on.
//!
//! This lives in the producer's library, beside the `settings.json` wiring,
//! for the same reason that does: the producer and the app must agree on these
//! paths exactly, and two copies of the resolution rules is how they stop
//! agreeing. Before this module they *were* two copies — `state_dir` existed
//! in both `statusline/src/main.rs` and `src-tauri/src/lib.rs`, with a comment
//! in each asking the reader to keep them in step by hand.
//!
//! Three rules, applied in order, by every resolver here:
//!
//! 1. A `SIMPLEBAR_*` override, if set. The escape hatch — see "Overrides" in
//!    DESIGN.md. It wins over XDG because `XDG_STATE_HOME` is a system-wide
//!    preference other tools read too, and redirecting SimpleBar alone should
//!    not mean moving everything else with it.
//! 2. The XDG variable, where one applies. Checked on Windows as well: it is
//!    not the platform's convention there, but someone who has deliberately
//!    set it has said what they want.
//! 3. The platform default — XDG's on Unix, `%LOCALAPPDATA%` on Windows.
//!
//! Every rule is expressed as a pure function over an environment lookup and a
//! [`Platform`], so both platforms' behaviour is testable from either one. The
//! public wrappers are thin: they supply the real environment and the real
//! platform, and hold no logic worth testing that the pure form does not.

use std::env;
use std::path::PathBuf;

/// Which set of conventions to follow.
///
/// An explicit value rather than `cfg!(windows)` inline, so a Mac can test the
/// Windows rules and vice versa. The port would otherwise be unverifiable
/// until someone booted the other operating system.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Platform {
    Unix,
    Windows,
}

impl Platform {
    /// The platform this binary was compiled for.
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }
}

/// The subdirectory both the state and config locations end in.
const APP_DIR: &str = "simplebar";

/// An environment variable that is set *and* non-empty.
///
/// An empty value reads as unset throughout. A caller who exports
/// `XDG_STATE_HOME=` has not chosen a directory, and treating "" as a path
/// would resolve the state directory to the filesystem root.
fn value(lookup: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    lookup(key).filter(|v| !v.is_empty())
}

/// The user's home directory.
///
/// `USERPROFILE` is the fallback because `HOME` is simply not set on Windows
/// outside a Unix-like shell. Checking `HOME` first still lets a Git Bash or
/// MSYS session — where both are set — resolve to the home that session
/// actually means.
fn home(lookup: &impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    value(lookup, "HOME")
        .or_else(|| value(lookup, "USERPROFILE"))
        .map(PathBuf::from)
}

/// `%LOCALAPPDATA%`, with a fallback for the case where it is missing.
///
/// It is always set on a real Windows session, but a stripped environment (a
/// service account, a CI runner, a `cmd` started with `/V` tricks) can lack
/// it. Reconstructing it from the profile is better than failing to find the
/// reading at all.
fn local_app_data(lookup: &impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    match value(lookup, "LOCALAPPDATA") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home(lookup).map(|h| h.join("AppData").join("Local")),
    }
}

/// The directory holding `usage.json` and `install.json`.
///
/// Note this is the final directory, not its parent: the override names the
/// place the files actually are, which is what someone setting it means.
pub fn state_dir_from(lookup: &impl Fn(&str) -> Option<String>, platform: Platform) -> Option<PathBuf> {
    if let Some(dir) = value(lookup, "SIMPLEBAR_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(xdg) = value(lookup, "XDG_STATE_HOME") {
        return Some(PathBuf::from(xdg).join(APP_DIR));
    }
    match platform {
        Platform::Windows => local_app_data(lookup).map(|d| d.join(APP_DIR)),
        Platform::Unix => home(lookup).map(|h| h.join(".local").join("state").join(APP_DIR)),
    }
}

/// The directory holding `config.json`.
///
/// On Windows this is the same directory as the state above. Windows draws no
/// distinction between state and configuration the way XDG does, and inventing
/// one would put two SimpleBar folders where the user expects one.
pub fn config_dir_from(lookup: &impl Fn(&str) -> Option<String>, platform: Platform) -> Option<PathBuf> {
    if let Some(dir) = value(lookup, "SIMPLEBAR_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(xdg) = value(lookup, "XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join(APP_DIR));
    }
    match platform {
        Platform::Windows => local_app_data(lookup).map(|d| d.join(APP_DIR)),
        Platform::Unix => home(lookup).map(|h| h.join(".config").join(APP_DIR)),
    }
}

/// Where the Connect button installs the producer.
///
/// Windows has no per-user directory on `PATH` to match `~/.local/bin`, and
/// does not need one: the installed producer is named in `settings.json` by
/// absolute path, so `PATH` never has to find it.
///
/// It sits under the same `APP_DIR` as the state and config directories rather
/// than a folder of its own. This used to be `SimpleBar\bin` beside
/// `simplebar\`, which on a case-insensitive filesystem is not beside anything
/// — the two were the same directory under two spellings, so which casing you
/// saw depended on which code created it first. One name, with `bin` inside it,
/// removes the question. Nothing moves on disk when this changes, for exactly
/// the reason it was confusing: the old and new paths are the same path.
pub fn bin_dir_from(lookup: &impl Fn(&str) -> Option<String>, platform: Platform) -> Option<PathBuf> {
    if let Some(dir) = value(lookup, "SIMPLEBAR_BIN_DIR") {
        return Some(PathBuf::from(dir));
    }
    match platform {
        Platform::Windows => local_app_data(lookup).map(|d| d.join(APP_DIR).join("bin")),
        Platform::Unix => home(lookup).map(|h| h.join(".local").join("bin")),
    }
}

/// Claude Code's `settings.json`.
///
/// The override takes a complete file path, not a directory, because the
/// reason to reach for it is that Claude Code has put the file somewhere this
/// code does not predict — and a directory override could not express that.
///
/// That Claude Code uses `%USERPROFILE%\.claude\settings.json` on Windows is
/// an assumption carried over from its macOS behaviour and is not yet
/// verified. `SIMPLEBAR_CLAUDE_SETTINGS` is the fix if it proves wrong, with
/// no rebuild needed.
pub fn settings_path_from(lookup: &impl Fn(&str) -> Option<String>) -> Result<PathBuf, String> {
    if let Some(path) = value(lookup, "SIMPLEBAR_CLAUDE_SETTINGS") {
        return Ok(PathBuf::from(path));
    }
    home(lookup)
        .map(|h| h.join(".claude").join("settings.json"))
        .ok_or_else(|| "no HOME or USERPROFILE in environment".to_string())
}

/// The producer's filename, which carries an extension on Windows.
///
/// Used both to write the binary out and to recognise an already-wired
/// command, so the two cannot disagree about the extension.
pub fn producer_file_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "simplebar-statusline.exe",
        Platform::Unix => "simplebar-statusline",
    }
}

/// Reads the real process environment. The one impure part of this module.
fn real_env(key: &str) -> Option<String> {
    env::var(key).ok()
}

/// The directory holding `usage.json` and `install.json`.
pub fn state_dir() -> Option<PathBuf> {
    state_dir_from(&real_env, Platform::current())
}

/// The directory holding `config.json`.
pub fn config_dir() -> Option<PathBuf> {
    config_dir_from(&real_env, Platform::current())
}

/// Where the Connect button installs the producer.
pub fn bin_dir() -> Option<PathBuf> {
    bin_dir_from(&real_env, Platform::current())
}

/// Claude Code's `settings.json`.
pub fn settings_path() -> Result<PathBuf, String> {
    settings_path_from(&real_env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// An environment built from pairs, so each test states exactly what is
    /// set and nothing else leaks in from the machine running the suite.
    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    const UNIX_HOME: (&str, &str) = ("HOME", "/home/dan");
    const WIN_HOME: (&str, &str) = ("USERPROFILE", r"C:\Users\dan");
    const WIN_LOCAL: (&str, &str) = ("LOCALAPPDATA", r"C:\Users\dan\AppData\Local");

    #[test]
    fn unix_defaults_follow_xdg() {
        let e = env(&[UNIX_HOME]);
        assert_eq!(
            state_dir_from(&e, Platform::Unix).unwrap(),
            PathBuf::from("/home/dan/.local/state/simplebar")
        );
        assert_eq!(
            config_dir_from(&e, Platform::Unix).unwrap(),
            PathBuf::from("/home/dan/.config/simplebar")
        );
        assert_eq!(
            bin_dir_from(&e, Platform::Unix).unwrap(),
            PathBuf::from("/home/dan/.local/bin")
        );
    }

    #[test]
    fn windows_defaults_follow_local_app_data() {
        let e = env(&[WIN_HOME, WIN_LOCAL]);
        assert_eq!(
            state_dir_from(&e, Platform::Windows).unwrap(),
            PathBuf::from(r"C:\Users\dan\AppData\Local").join("simplebar")
        );
        // State and config share one folder on Windows, by decision.
        assert_eq!(
            config_dir_from(&e, Platform::Windows).unwrap(),
            state_dir_from(&e, Platform::Windows).unwrap()
        );
        // Under the state folder, not a second folder differing only in case.
        assert_eq!(
            bin_dir_from(&e, Platform::Windows).unwrap(),
            PathBuf::from(r"C:\Users\dan\AppData\Local")
                .join("simplebar")
                .join("bin")
        );
    }

    /// The Windows locations must differ by more than capitalisation, since the
    /// filesystem there does not distinguish them. Asserted case-insensitively
    /// so the failure names the real problem — two directories that are one
    /// directory — rather than an unequal string.
    #[test]
    fn the_windows_bin_directory_is_not_the_state_directory_in_disguise() {
        let e = env(&[WIN_HOME, WIN_LOCAL]);
        let state = state_dir_from(&e, Platform::Windows).unwrap();
        let bin = bin_dir_from(&e, Platform::Windows).unwrap();
        assert_ne!(
            state.to_string_lossy().to_lowercase(),
            bin.to_string_lossy().to_lowercase()
        );
        assert!(bin.starts_with(&state));
    }

    #[test]
    fn windows_without_localappdata_falls_back_to_the_profile() {
        let e = env(&[WIN_HOME]);
        assert_eq!(
            state_dir_from(&e, Platform::Windows).unwrap(),
            PathBuf::from(r"C:\Users\dan")
                .join("AppData")
                .join("Local")
                .join("simplebar")
        );
    }

    #[test]
    fn simplebar_override_beats_xdg_and_the_platform_default() {
        let e = env(&[
            UNIX_HOME,
            WIN_LOCAL,
            ("XDG_STATE_HOME", "/xdg/state"),
            ("SIMPLEBAR_STATE_DIR", "/tmp/sb"),
        ]);
        // Same answer on both platforms: an explicit path is an explicit path.
        for platform in [Platform::Unix, Platform::Windows] {
            assert_eq!(
                state_dir_from(&e, platform).unwrap(),
                PathBuf::from("/tmp/sb")
            );
        }
    }

    #[test]
    fn the_override_names_the_directory_itself_not_its_parent() {
        // XDG gets `simplebar` appended; an explicit override does not, because
        // it names the place the files actually are.
        let xdg = env(&[UNIX_HOME, ("XDG_STATE_HOME", "/xdg/state")]);
        assert_eq!(
            state_dir_from(&xdg, Platform::Unix).unwrap(),
            PathBuf::from("/xdg/state/simplebar")
        );

        let explicit = env(&[UNIX_HOME, ("SIMPLEBAR_STATE_DIR", "/tmp/sb")]);
        assert_eq!(
            state_dir_from(&explicit, Platform::Unix).unwrap(),
            PathBuf::from("/tmp/sb")
        );
    }

    #[test]
    fn xdg_is_honoured_on_windows_too() {
        let e = env(&[WIN_HOME, WIN_LOCAL, ("XDG_CONFIG_HOME", "/xdg/config")]);
        assert_eq!(
            config_dir_from(&e, Platform::Windows).unwrap(),
            PathBuf::from("/xdg/config/simplebar")
        );
    }

    #[test]
    fn an_empty_variable_reads_as_unset() {
        // Not the filesystem root, which is what taking "" literally would give.
        let e = env(&[UNIX_HOME, ("XDG_STATE_HOME", ""), ("SIMPLEBAR_STATE_DIR", "")]);
        assert_eq!(
            state_dir_from(&e, Platform::Unix).unwrap(),
            PathBuf::from("/home/dan/.local/state/simplebar")
        );
    }

    #[test]
    fn settings_default_to_dot_claude_in_the_home_directory() {
        let unix = env(&[UNIX_HOME]);
        assert_eq!(
            settings_path_from(&unix).unwrap(),
            PathBuf::from("/home/dan/.claude/settings.json")
        );

        // No HOME on Windows; USERPROFILE carries it.
        let windows = env(&[WIN_HOME]);
        assert_eq!(
            settings_path_from(&windows).unwrap(),
            PathBuf::from(r"C:\Users\dan")
                .join(".claude")
                .join("settings.json")
        );
    }

    #[test]
    fn home_wins_over_userprofile_when_both_are_set() {
        // A Git Bash session sets both; HOME is the one that session means.
        let e = env(&[UNIX_HOME, WIN_HOME]);
        assert_eq!(
            settings_path_from(&e).unwrap(),
            PathBuf::from("/home/dan/.claude/settings.json")
        );
    }

    #[test]
    fn settings_override_takes_a_whole_file_path() {
        let e = env(&[
            UNIX_HOME,
            ("SIMPLEBAR_CLAUDE_SETTINGS", "/somewhere/else/settings.json"),
        ]);
        assert_eq!(
            settings_path_from(&e).unwrap(),
            PathBuf::from("/somewhere/else/settings.json")
        );
    }

    #[test]
    fn settings_fail_when_there_is_no_home_at_all() {
        let e = env(&[]);
        assert!(settings_path_from(&e).is_err());
    }

    #[test]
    fn the_producer_is_an_exe_on_windows_only() {
        assert_eq!(producer_file_name(Platform::Windows), "simplebar-statusline.exe");
        assert_eq!(producer_file_name(Platform::Unix), "simplebar-statusline");
    }
}
