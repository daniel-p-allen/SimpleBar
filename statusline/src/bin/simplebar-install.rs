//! Wires the producer into Claude Code — M8.
//!
//! Claude Code reads a `statusLine` command from `~/.claude/settings.json` and
//! runs it on every status-line render. This installer sets that key to point
//! at the built producer binary, so a user does not have to hand-edit JSON.
//!
//! Unlike the producer, this is an ordinary CLI: it either succeeds and says
//! so, or it fails loudly on stderr with a non-zero exit. It deliberately
//! refuses to touch a `settings.json` it cannot parse — overwriting a config
//! we do not understand would lose whatever else the user keeps in it.
//!
//! The wired path is resolved from this binary's own location: the producer is
//! its sibling in the same build directory. That keeps the command correct
//! wherever the repository lives, with nothing hard-coded.

use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn main() {
    if let Err(message) = install() {
        eprintln!("install-statusline: {message}");
        process::exit(1);
    }
}

fn install() -> Result<(), String> {
    let command_path = producer_path()?;
    let settings = settings_path()?;

    // None when the file does not exist yet — a first-time setup. Any other
    // read error (a directory, bad permissions) is a real failure to report.
    let existing = match fs::read_to_string(&settings) {
        Ok(raw) => Some(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", settings.display())),
    };

    let merged = merged_settings(existing.as_deref(), &command_path)?;

    let dir = settings
        .parent()
        .ok_or("settings path has no parent directory")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    // Back up whatever was there before touching it, so a surprised user can
    // put it back. Only when a file existed — there is nothing to preserve on
    // a first-time setup.
    if existing.is_some() {
        let backup = settings.with_extension("json.bak");
        fs::copy(&settings, &backup)
            .map_err(|e| format!("cannot write backup {}: {e}", backup.display()))?;
    }

    // Temp file plus rename, as the rest of the app writes JSON — a settings
    // file half-written by a crash would read as malformed on next launch.
    let encoded = serde_json::to_vec_pretty(&merged).map_err(|e| e.to_string())?;
    let tmp = settings.with_extension("json.tmp");
    fs::write(&tmp, encoded).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &settings).map_err(|e| e.to_string())?;

    println!("Wired statusLine to {command_path}");
    println!("in {}", settings.display());
    if existing.is_some() {
        println!("(previous settings backed up to settings.json.bak)");
    }
    Ok(())
}

/// The producer binary that sits beside this installer in the build directory.
/// Verified to exist so we never wire a path Claude Code cannot run.
fn producer_path() -> Result<String, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe
        .parent()
        .ok_or("installer has no parent directory")?;
    let producer = dir.join("simplebar-statusline");
    if !producer.exists() {
        return Err(format!(
            "producer not found at {} — run `make build` first",
            producer.display()
        ));
    }
    producer
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "producer path is not valid UTF-8".to_string())
}

fn settings_path() -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| "no HOME in environment")?;
    Ok(Path::new(&home).join(".claude").join("settings.json"))
}

/// Pure merge: takes the existing settings text (None if the file is absent)
/// and returns the settings with `statusLine` set to run `command_path`,
/// leaving every other key untouched. Errors rather than clobbering when the
/// existing text is not a JSON object.
fn merged_settings(existing: Option<&str>, command_path: &str) -> Result<Value, String> {
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
