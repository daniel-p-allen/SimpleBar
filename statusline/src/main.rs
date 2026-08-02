//! The SimpleBar producer — M1.
//!
//! Claude Code pipes its status line JSON blob to this binary on stdin. We
//! pull out the 5-hour subscription window, write it where SimpleBar will
//! watch for it, and print a one-line status.
//!
//! Deliberately silent on failure: `rate_limits` is absent for
//! non-subscribers and before the first API response of a session, and the
//! schema is Claude Code's internal contract. A status line that crashes or
//! prints a stack trace is worse than one that prints nothing, so anything
//! unexpected leaves the last good usage.json alone and exits quietly.
//!
//! Follows the CLI Guidelines (https://clig.dev/): the status line text is
//! the primary output and goes to stdout, nothing is written to stderr, and
//! we exit 0. The deliberate deviation is that we exit 0 on bad input too —
//! a status line is not a normal CLI consumer, and a non-zero exit here
//! would be noise nobody acts on.

use chrono::{Local, TimeZone};
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct Usage {
    used_percentage: f64,
    resets_at: i64,
    written_at: i64,
    model_name: Option<String>,
}

/// `$XDG_STATE_HOME`, defaulting to `~/.local/state`.
///
/// SimpleBar is both a CLI (this producer) and a GUI (the Tauri app). macOS
/// convention would put GUI data under `~/Library/Application Support`, but
/// the two halves share this one file and the producer is the half that
/// runs in a terminal — so we follow XDG, which also makes a Linux port
/// free later.
fn state_dir() -> Option<PathBuf> {
    if let Ok(xdg) = env::var("XDG_STATE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    env::var("HOME").ok().map(|home| PathBuf::from(home).join(".local/state"))
}

fn main() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    let Ok(blob) = serde_json::from_str::<Value>(&input) else {
        return;
    };

    // Valid JSON is not necessarily an object — a bare list or string
    // parses fine and would then have nothing to index into.
    let Some(rate_limits) = blob.get("rate_limits").and_then(Value::as_object) else {
        return;
    };
    let Some(five_hour) = rate_limits.get("five_hour").and_then(Value::as_object) else {
        return;
    };
    let Some(used) = five_hour.get("used_percentage").and_then(Value::as_f64) else {
        return;
    };
    let Some(resets_at) = five_hour.get("resets_at").and_then(Value::as_i64) else {
        return;
    };

    let remaining = 100.0 - used;

    // Purely cosmetic — a small label under the reset time in the app. Never
    // blocks the required fields above; absent or malformed model info just
    // means no label.
    let model_name = blob
        .get("model")
        .and_then(Value::as_object)
        .and_then(|m| m.get("display_name"))
        .and_then(Value::as_str)
        .map(String::from);

    // 12-hour with am/pm — "1:00 am", not "01:00". No leading zero on the
    // hour ("%-I" drops the pad).
    let resets_str = Local
        .timestamp_opt(resets_at, 0)
        .single()
        .map(|dt| dt.format("%-I:%M %p").to_string().to_lowercase());

    if let Some(dir) = state_dir() {
        let out_dir = dir.join("simplebar");
        let out_file = out_dir.join("usage.json");
        let written_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let usage = Usage { used_percentage: used, resets_at, written_at, model_name };

        // Atomic write — temp file plus rename — so a reader never sees a
        // half-written file. Never let a write failure take out the status
        // line.
        let _ = fs::create_dir_all(&out_dir).and_then(|_| {
            let tmp = out_dir.join("usage.json.tmp");
            fs::write(&tmp, serde_json::to_vec(&usage).unwrap_or_default())?;
            fs::rename(&tmp, &out_file)
        });
    }

    if let Some(resets_str) = resets_str {
        println!("{:.0}% left · resets {}", remaining, resets_str);
    }
}
