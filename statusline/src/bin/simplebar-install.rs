//! Wires the producer into Claude Code — M8.
//!
//! The developer's install route: run from a source checkout, it registers the
//! producer built alongside it. The app's Connect button does the same job for
//! someone who downloaded the DMG and has no checkout — both go through
//! `simplebar_statusline::wire`, so neither can grow a guard the other lacks.
//!
//! Unlike the producer, this is an ordinary CLI: it either succeeds and says
//! so, or it fails loudly on stderr with a non-zero exit. It deliberately
//! refuses to touch a `settings.json` it cannot parse — overwriting a config
//! we do not understand would lose whatever else the user keeps in it.
//!
//! The wired path is resolved from this binary's own location: the producer is
//! its sibling in the same build directory. That keeps the command correct
//! wherever the repository lives, with nothing hard-coded.

use std::env;
use std::process;

fn main() {
    if let Err(message) = install() {
        eprintln!("install-statusline: {message}");
        process::exit(1);
    }
}

fn install() -> Result<(), String> {
    let command_path = producer_path()?;
    let wired = simplebar_statusline::wire(&command_path)?;

    println!("Wired statusLine to {command_path}");
    println!("in {}", wired.settings.display());
    if wired.backed_up {
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
    let producer = dir.join(simplebar_statusline::paths::producer_file_name(
        simplebar_statusline::paths::Platform::current(),
    ));
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
