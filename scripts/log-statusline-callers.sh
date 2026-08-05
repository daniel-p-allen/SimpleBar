#!/usr/bin/env bash
#
# Which Claude surfaces actually invoke the statusLine command?
#
# The obvious test — watch usage.json change — cannot answer it. Any open
# Claude Code session rewrites that file on every render, so a change while
# testing VS Code may simply be the terminal session that is running the test.
# Attribution needs something that identifies the caller.
#
# This wraps the real producer: it appends one line per invocation recording who
# called, then hands stdin on unchanged, so the gauge keeps working throughout.
# The blob carries `session_id`, `version` and the working directory, and those
# differ between surfaces — a VS Code panel opened on another folder, or the
# desktop app, shows up as a distinct session.
#
# Install:   scripts/log-statusline-callers.sh install
# Read:      scripts/log-statusline-callers.sh report
# Remove:    scripts/log-statusline-callers.sh uninstall
#
# Uninstall restores the previous statusLine command, so this is a temporary
# diagnostic rather than a change to how SimpleBar is wired.

set -euo pipefail

LOG="${TMPDIR:-/tmp}/simplebar-statusline-callers.log"
SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
SETTINGS="$HOME/.claude/settings.json"
SAVED="$HOME/.simplebar-statusline-before-logging"

# The wrapper itself: called by Claude Code with the blob on stdin.
run_wrapper() {
    blob=$(cat)

    # Fields that identify the caller. Kept to one line so the log stays
    # readable, and failing quietly — a diagnostic must never break the status
    # line it is observing.
    printf '%s\t%s\n' "$(date '+%H:%M:%S')" "$(
        printf '%s' "$blob" | node -e '
            let raw = "";
            process.stdin.on("data", d => raw += d);
            process.stdin.on("end", () => {
                try {
                    const b = JSON.parse(raw);
                    const dir = b.workspace?.current_dir ?? b.cwd ?? "?";
                    const rl = b.rate_limits?.five_hour ? "rate_limits:yes" : "rate_limits:NO";
                    process.stdout.write(
                        `session=${(b.session_id ?? "?").slice(0, 8)} ` +
                        `version=${b.version ?? "?"} ` +
                        `model=${b.model?.display_name ?? "?"} ` +
                        `${rl} dir=${dir}`
                    );
                } catch (e) {
                    process.stdout.write("unparseable blob");
                }
            });
        ' 2>/dev/null || echo "log failed"
    )" >>"$LOG" 2>/dev/null || true

    # Hand the blob to the real producer untouched, so the gauge keeps updating
    # normally while the log is being collected.
    producer=$(cat "$SAVED" 2>/dev/null || true)
    if [ -n "$producer" ] && [ -x "$producer" ]; then
        printf '%s' "$blob" | "$producer"
    fi
}

install_wrapper() {
    current=$(node -e '
        const fs = require("fs");
        try {
            const s = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
            process.stdout.write(s?.statusLine?.command ?? "");
        } catch (e) { process.stdout.write(""); }
    ' "$SETTINGS")

    [ -n "$current" ] || { echo "No statusLine command wired — nothing to wrap."; exit 1; }
    case "$current" in
    *log-statusline-callers.sh) echo "Already installed. Run 'report' or 'uninstall'."; exit 0 ;;
    esac

    printf '%s' "$current" >"$SAVED"
    cp "$SETTINGS" "$SETTINGS.before-logging.bak"

    node -e '
        const fs = require("fs");
        const [path, cmd] = process.argv.slice(1);
        const s = JSON.parse(fs.readFileSync(path, "utf8"));
        s.statusLine = { type: "command", command: cmd };
        fs.writeFileSync(path, JSON.stringify(s, null, 2) + "\n");
    ' "$SETTINGS" "$SELF run"

    : >"$LOG"
    echo "Wrapped. Real producer saved: $current"
    echo "Log: $LOG"
    echo
    echo "Now send a prompt in each surface you want to test, then run:"
    echo "  $0 report"
}

report() {
    [ -f "$LOG" ] || { echo "No log yet at $LOG"; exit 1; }
    echo "Every statusLine invocation recorded:"
    echo
    cat "$LOG"
    echo
    echo "Distinct sessions seen:"
    awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^session=/) print $i }' "$LOG" | sort -u
}

uninstall_wrapper() {
    producer=$(cat "$SAVED" 2>/dev/null || true)
    [ -n "$producer" ] || { echo "Nothing saved to restore."; exit 1; }

    node -e '
        const fs = require("fs");
        const [path, cmd] = process.argv.slice(1);
        const s = JSON.parse(fs.readFileSync(path, "utf8"));
        s.statusLine = { type: "command", command: cmd };
        fs.writeFileSync(path, JSON.stringify(s, null, 2) + "\n");
    ' "$SETTINGS" "$producer"

    rm -f "$SAVED"
    echo "Restored statusLine to $producer"
}

case "${1:-}" in
run) run_wrapper ;;
install) install_wrapper ;;
report) report ;;
uninstall) uninstall_wrapper ;;
*)
    echo "Usage: $0 {install|report|uninstall}"
    exit 1
    ;;
esac
