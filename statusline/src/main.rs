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
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct Usage {
    used_percentage: f64,
    resets_at: i64,
    written_at: i64,
    model_name: Option<String>,
    effort_level: Option<String>,
}

/// Pulls a reading out of a status line blob.
///
/// `None` means "nothing usable here" and is the ordinary case, not an error:
/// `rate_limits` is absent for non-subscribers and before the first API
/// response of a session. Every `None` below leaves the last good usage.json
/// alone.
///
/// `written_at` is passed in rather than read from the clock so this stays a
/// pure function, testable without freezing time.
fn reading_from_blob(blob: &Value, written_at: i64) -> Option<Usage> {
    // Valid JSON is not necessarily an object — a bare list or string
    // parses fine and would then have nothing to index into.
    let rate_limits = blob.get("rate_limits").and_then(Value::as_object)?;
    let five_hour = rate_limits.get("five_hour").and_then(Value::as_object)?;
    let used = five_hour.get("used_percentage").and_then(Value::as_f64)?;
    let resets_at = five_hour.get("resets_at").and_then(Value::as_i64)?;

    // Purely cosmetic — a small label under the reset time in the app. Never
    // blocks the required fields above; absent or malformed model info just
    // means no label.
    let model_name = blob
        .get("model")
        .and_then(Value::as_object)
        .and_then(|m| m.get("display_name"))
        .and_then(Value::as_str)
        .map(String::from);

    // Stored exactly as the blob gives it — lower case. Casing for display is
    // the consumer's business, so the file stays a record of what was read
    // rather than of how it will be shown.
    let effort_level = blob
        .get("effort")
        .and_then(Value::as_object)
        .and_then(|e| e.get("level"))
        .and_then(Value::as_str)
        .map(String::from);

    Some(Usage { used_percentage: used, resets_at, written_at, model_name, effort_level })
}

/// Parses raw stdin and pulls the reading out of it.
///
/// Wraps [`reading_from_blob`] with the JSON parse so that malformed input and
/// empty stdin — both ordinary, both named in DESIGN.md — are handled in one
/// testable place rather than inline in `main`.
fn reading_from_input(input: &str, written_at: i64) -> Option<Usage> {
    let blob = serde_json::from_str::<Value>(input).ok()?;
    reading_from_blob(&blob, written_at)
}

/// Writes the reading atomically: temp file, then rename.
///
/// The rename is what makes it atomic — the watcher either sees the old file
/// or the new one, never a half-written one. The temp file sits in the same
/// directory as the target because rename is only atomic within a filesystem.
fn write_reading(dir: &Path, usage: &Usage) -> io::Result<()> {
    fs::create_dir_all(dir)?;

    let tmp = dir.join("usage.json.tmp");
    let encoded = serde_json::to_vec(usage)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, encoded)?;
    fs::rename(&tmp, dir.join("usage.json"))
}

/// The status line text: "83% left · resets 12:40 am".
///
/// `None` when the timestamp has no single local time — a nonsensical epoch,
/// or the ambiguous hour when clocks go back. The reading is still written in
/// that case; only the printed line is skipped.
fn status_line(remaining: f64, resets_at: i64) -> Option<String> {
    // 12-hour with am/pm — "1:00 am", not "01:00". No leading zero on the
    // hour ("%-I" drops the pad).
    let resets_str = Local
        .timestamp_opt(resets_at, 0)
        .single()
        .map(|dt| dt.format("%-I:%M %p").to_string().to_lowercase())?;

    Some(format!("{remaining:.0}% left · resets {resets_str}"))
}

fn main() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    let written_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Nothing usable in the blob: leave the last good usage.json alone and say
    // nothing. This is the ordinary case, not a failure.
    let Some(usage) = reading_from_input(&input, written_at) else {
        return;
    };
    let remaining = 100.0 - usage.used_percentage;
    let resets_at = usage.resets_at;

    if let Some(dir) = simplebar_statusline::paths::state_dir() {
        // A write failure must never take out the status line.
        let _ = write_reading(&dir, &usage);
    }

    if let Some(line) = status_line(remaining, resets_at) {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests need these: the fixture loader builds paths, and the
    // write tests need a temp directory. The producer itself is handed the
    // directory to write into and reads no environment of its own.
    use std::env;
    use std::path::PathBuf;

    /// The fixtures are shared with the consumer's tests, so they live at the
    /// repo root rather than inside this crate.
    fn fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name);
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing fixture {name}: {e}"))
    }

    // The happy path, and the two cosmetic fields that ride along with it.

    #[test]
    fn full_blob_yields_the_five_hour_reading() {
        let usage = reading_from_blob(&fixture("full.json"), 1234).expect("a reading");

        assert_eq!(usage.used_percentage, 65.0);
        assert_eq!(usage.resets_at, 1785682800);
        assert_eq!(usage.written_at, 1234, "written_at is ours, passed in, not read from the clock");
        assert_eq!(usage.model_name.as_deref(), Some("Opus 5"));
    }

    #[test]
    fn seven_day_is_ignored() {
        // Both windows are present in the fixture. Reading the wrong one would
        // still produce a plausible-looking number, so this is worth pinning.
        let usage = reading_from_blob(&fixture("full.json"), 0).expect("a reading");
        assert_eq!(usage.used_percentage, 65.0, "should be five_hour's 65, not seven_day's 12");
    }

    #[test]
    fn effort_level_is_kept_lower_case() {
        let blob = serde_json::json!({
            "rate_limits": { "five_hour": { "used_percentage": 10, "resets_at": 1 } },
            "effort": { "level": "medium" },
        });
        let usage = reading_from_blob(&blob, 0).expect("a reading");
        assert_eq!(
            usage.effort_level.as_deref(),
            Some("medium"),
            "casing is the consumer's business; the file records what was read"
        );
    }

    #[test]
    fn missing_cosmetic_fields_do_not_block_the_reading() {
        let usage = reading_from_blob(&fixture("full_no_model.json"), 0)
            .expect("a reading despite no model");
        assert_eq!(usage.used_percentage, 65.0);
        assert_eq!(usage.model_name, None);
        assert_eq!(usage.effort_level, None);
    }

    // Everything below must yield None, leaving the last good usage.json alone.

    #[test]
    fn no_rate_limits_yields_nothing() {
        // The ordinary case for a non-subscriber, and before the first API
        // response of a session — not an error.
        assert!(reading_from_blob(&fixture("no_rate_limits.json"), 0).is_none());
    }

    #[test]
    fn five_hour_missing_yields_nothing() {
        // seven_day is present in this fixture. Falling back to it would
        // silently show the wrong window.
        assert!(reading_from_blob(&fixture("five_hour_missing.json"), 0).is_none());
    }

    #[test]
    fn json_that_is_not_an_object_yields_nothing() {
        // A bare list or string parses as valid JSON and has nothing to index.
        for raw in ["[]", "\"a string\"", "42", "null"] {
            let blob: Value = serde_json::from_str(raw).expect("valid json");
            assert!(reading_from_blob(&blob, 0).is_none(), "{raw} should yield nothing");
        }
    }

    #[test]
    fn wrongly_typed_fields_yield_nothing() {
        // The schema is Claude Code's internal contract and can change. A
        // string where a number belongs must not panic or write garbage.
        let blob = serde_json::json!({
            "rate_limits": { "five_hour": { "used_percentage": "65", "resets_at": 1 } },
        });
        assert!(reading_from_blob(&blob, 0).is_none());

        let blob = serde_json::json!({
            "rate_limits": { "five_hour": { "used_percentage": 65, "resets_at": "soon" } },
        });
        assert!(reading_from_blob(&blob, 0).is_none());
    }

    #[test]
    fn wrongly_typed_cosmetic_fields_are_dropped_not_fatal() {
        let blob = serde_json::json!({
            "rate_limits": { "five_hour": { "used_percentage": 65, "resets_at": 1 } },
            "model": "Opus 5",          // a string, not the expected object
            "effort": { "level": 3 },   // a number, not a string
        });
        let usage = reading_from_blob(&blob, 0).expect("the reading still stands");
        assert_eq!(usage.model_name, None);
        assert_eq!(usage.effort_level, None);
    }

    // Raw input, before it is JSON at all. DESIGN.md names both of these.

    #[test]
    fn empty_stdin_yields_nothing() {
        // Claude Code can invoke the status line with nothing to say.
        assert!(reading_from_input("", 0).is_none());
        assert!(reading_from_input("   \n\t ", 0).is_none());
    }

    #[test]
    fn malformed_json_yields_nothing() {
        let raw = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/malformed.json"),
        )
        .expect("the malformed fixture");
        assert!(reading_from_input(&raw, 0).is_none(), "truncated JSON must not panic");
    }

    #[test]
    fn well_formed_input_round_trips() {
        // The positive case for the parse wrapper, so the two tests above are
        // known to fail for the right reason rather than always returning None.
        let raw = r#"{"rate_limits":{"five_hour":{"used_percentage":40,"resets_at":99}}}"#;
        let usage = reading_from_input(raw, 7).expect("a reading");
        assert_eq!(usage.used_percentage, 40.0);
        assert_eq!(usage.resets_at, 99);
        assert_eq!(usage.written_at, 7);
    }

    // The write. DESIGN.md: "the write is atomic, and a bad input leaves any
    // existing file untouched."

    /// A unique empty directory under the system temp dir. Avoids pulling in a
    /// tempfile dependency for the sake of four tests.
    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = env::temp_dir().join(format!("simplebar-test-{tag}-{unique}"));
        fs::create_dir_all(&dir).expect("a temp dir");
        dir
    }

    fn sample(used: f64) -> Usage {
        Usage {
            used_percentage: used,
            resets_at: 1785768000,
            written_at: 1,
            model_name: Some("Opus 5".into()),
            effort_level: Some("medium".into()),
        }
    }

    #[test]
    fn write_creates_the_file_and_leaves_no_temp_behind() {
        let dir = temp_dir("write");
        write_reading(&dir, &sample(65.0)).expect("the write");

        let written = fs::read_to_string(dir.join("usage.json")).expect("usage.json");
        assert!(written.contains("\"used_percentage\":65"), "got {written}");
        assert!(
            !dir.join("usage.json.tmp").exists(),
            "the temp file must be renamed away, not left for the watcher to find"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_creates_the_directory_when_missing() {
        // A fresh install has no state directory yet.
        let dir = temp_dir("mkdir").join("nested/simplebar");
        write_reading(&dir, &sample(10.0)).expect("the write");
        assert!(dir.join("usage.json").exists());

        fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let dir = temp_dir("replace");
        write_reading(&dir, &sample(10.0)).expect("first write");
        write_reading(&dir, &sample(90.0)).expect("second write");

        let written = fs::read_to_string(dir.join("usage.json")).expect("usage.json");
        assert!(written.contains("\"used_percentage\":90"), "got {written}");
        assert!(!written.contains("\"used_percentage\":10"), "stale reading survived: {written}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bad_input_leaves_an_existing_reading_untouched() {
        // The whole failure story in one test: a good reading is on disk, bad
        // input arrives, and the file must still hold the good reading. The
        // producer achieves this by never reaching the write at all, which is
        // what the None below stands for.
        let dir = temp_dir("preserve");
        write_reading(&dir, &sample(65.0)).expect("the good write");

        for bad in ["", "{ not json", "[]", r#"{"rate_limits":{}}"#] {
            assert!(reading_from_input(bad, 0).is_none(), "{bad:?} should yield nothing");
        }

        let written = fs::read_to_string(dir.join("usage.json")).expect("usage.json");
        assert!(written.contains("\"used_percentage\":65"), "good reading lost: {written}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn status_line_reads_as_remaining_not_used() {
        // 17% used is 83% left. Printing the used figure would be wrong in the
        // most confusing possible way — plausible, and backwards.
        let line = status_line(83.0, 1785768000).expect("a line");
        assert!(line.starts_with("83% left"), "got {line:?}");
    }
}
