//! Wiring the producer into Claude Code's `settings.json`.
//!
//! Claude Code reads a `statusLine` command from `~/.claude/settings.json` and
//! runs it on every status-line render. Setting that key is the one act that
//! turns a downloaded app into a working gauge, and there are two callers for
//! it: the `simplebar-install` CLI, and the app's Connect button.
//!
//! It lives here, in a library, rather than in either of them. Two copies of
//! code that edits *another tool's* config file is how the two drift apart —
//! one gaining a backup or a malformed-file guard that the other never gets.
//!
//! The guiding rule throughout: never clobber what we do not understand. A
//! `settings.json` that fails to parse is left exactly as it was, and the whole
//! operation fails, rather than replacing a file the user may keep other
//! settings in.

use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// What a successful wiring did, for the caller to report.
pub struct Wired {
    /// The file that was written.
    pub settings: PathBuf,
    /// Whether a previous file was backed up first. False on a first-time
    /// setup, where there was nothing to preserve.
    pub backed_up: bool,
}

/// `~/.claude/settings.json`.
pub fn settings_path() -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| "no HOME in environment")?;
    Ok(Path::new(&home).join(".claude").join("settings.json"))
}

/// The `statusLine` command currently registered, if there is one.
///
/// Deliberately lenient in a way [`wire`] is not: every failure — no file, bad
/// permissions, malformed JSON, a `statusLine` of the wrong shape — reports
/// `None`. The only caller asks "is SimpleBar wired?", and for that question an
/// unreadable settings file and an absent one mean the same thing: not as far
/// as we can tell. Refusing to answer would leave the UI with nothing to show.
///
/// Note the asymmetry is safe because this never writes. Wiring *into* a file
/// we cannot parse still fails loudly.
pub fn wired_command() -> Option<String> {
    let raw = fs::read_to_string(settings_path().ok()?).ok()?;
    let root: Value = serde_json::from_str(&raw).ok()?;
    root.get("statusLine")?
        .get("command")?
        .as_str()
        .map(str::to_owned)
}

/// Points Claude Code's `statusLine` at `command_path`, preserving everything
/// else in the file.
///
/// Backs up first, then writes atomically — temp file plus rename, as the rest
/// of the app writes JSON. A settings file half-written by a crash would read
/// as malformed on the next launch, which for a config we did not author is a
/// bad way to leave someone.
pub fn wire(command_path: &str) -> Result<Wired, String> {
    let settings = settings_path()?;

    // None when the file does not exist yet — a first-time setup. Any other
    // read error (a directory, bad permissions) is a real failure to report.
    let existing = match fs::read_to_string(&settings) {
        Ok(raw) => Some(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", settings.display())),
    };

    // Merged before anything is touched, so a malformed file fails without
    // having created a directory or written a backup.
    let merged = merged_settings(existing.as_deref(), command_path)?;

    let dir = settings
        .parent()
        .ok_or("settings path has no parent directory")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    // Back up whatever was there before touching it, so a surprised user can
    // put it back.
    let backed_up = existing.is_some();
    if backed_up {
        let backup = settings.with_extension("json.bak");
        fs::copy(&settings, &backup)
            .map_err(|e| format!("cannot write backup {}: {e}", backup.display()))?;
    }

    let encoded = serde_json::to_vec_pretty(&merged).map_err(|e| e.to_string())?;
    let tmp = settings.with_extension("json.tmp");
    fs::write(&tmp, encoded).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &settings).map_err(|e| e.to_string())?;

    Ok(Wired { settings, backed_up })
}

/// Pure merge: takes the existing settings text (None if the file is absent)
/// and returns the settings with `statusLine` set to run `command_path`,
/// leaving every other key untouched. Errors rather than clobbering when the
/// existing text is not a JSON object.
pub fn merged_settings(existing: Option<&str>, command_path: &str) -> Result<Value, String> {
    let mut root = match existing {
        None => Value::Object(Default::default()),
        Some(raw) if raw.trim().is_empty() => Value::Object(Default::default()),
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| format!("existing settings.json is not valid JSON ({e}); left unchanged"))?,
    };

    let object = root
        .as_object_mut()
        .ok_or("existing settings.json is not a JSON object; left unchanged")?;

    object.insert(
        "statusLine".to_string(),
        json!({ "type": "command", "command": command_path }),
    );

    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/repo/statusline/target/release/simplebar-statusline";

    #[test]
    fn adds_statusline_and_keeps_other_keys() {
        let existing = r#"{ "theme": "dark", "model": "opus" }"#;
        let out = merged_settings(Some(existing), CMD).unwrap();
        // Untouched keys survive.
        assert_eq!(out["theme"], "dark");
        assert_eq!(out["model"], "opus");
        // statusLine is wired.
        assert_eq!(out["statusLine"]["type"], "command");
        assert_eq!(out["statusLine"]["command"], CMD);
    }

    #[test]
    fn replaces_an_existing_statusline_only() {
        let existing = r#"{ "theme": "dark",
            "statusLine": { "type": "command", "command": "/old/path" } }"#;
        let out = merged_settings(Some(existing), CMD).unwrap();
        assert_eq!(out["statusLine"]["command"], CMD);
        assert_eq!(out["theme"], "dark");
    }

    #[test]
    fn missing_file_produces_settings_with_just_statusline() {
        let out = merged_settings(None, CMD).unwrap();
        assert_eq!(out["statusLine"]["command"], CMD);
        assert_eq!(out.as_object().unwrap().len(), 1);
    }

    #[test]
    fn empty_file_is_treated_as_no_settings() {
        let out = merged_settings(Some("   \n"), CMD).unwrap();
        assert_eq!(out["statusLine"]["command"], CMD);
    }

    #[test]
    fn malformed_json_errors_and_does_not_clobber() {
        let out = merged_settings(Some("{ not json"), CMD);
        assert!(out.is_err());
    }

    #[test]
    fn non_object_json_errors() {
        // Valid JSON, but an array — replacing it would lose the user's file.
        let out = merged_settings(Some("[1, 2, 3]"), CMD);
        assert!(out.is_err());
    }
}
