#!/usr/bin/env bash
#
# Make this machine look like it has never installed SimpleBar, so the
# Connect button can be tested the way a new user meets it — and put it back
# afterwards.
#
# Why it exists: a developer's machine is wired by `make install-statusline`,
# and that deliberately counts as connected, so the button never appears. But
# that is only the most obvious difference. A machine that has run this app
# before also carries saved preferences, a previous reading, and an installed
# bundle — so a "fresh" test that leaves those in place still opens a window
# sized and positioned to taste, showing a live number, which is nothing like
# what a new user meets. Everything below goes:
#
#   * the installed app at /Applications/SimpleBar.app
#   * the copied producer at ~/.local/bin/simplebar-statusline
#   * the state directory: the reading AND the setup record
#   * preferences: mute, pin, and the remembered window geometry
#   * the statusLine entry in ~/.claude/settings.json
#
# It also quarantines the DMG, so Gatekeeper refuses the first launch exactly
# as it would on a real download. That step is the one most worth rehearsing:
# it is where a new user gets stuck, and it cannot be removed while the app is
# unsigned.
#
# THIS EDITS ~/.claude/settings.json, which is Claude Code's own config and
# nothing to do with this repo. Every run backs it up first, to a timestamped
# file in $HOME that nothing here ever deletes, and `reset` refuses to proceed
# without a typed confirmation. Restoring is a copy back from that backup.
#
# Only the statusLine key is touched; every other setting is preserved, using
# the same read-modify-write shape the installer uses.
#
# Portability: macOS ships bash 3.2. Node is already a build requirement, so it
# does the JSON editing rather than assuming jq is installed.

set -euo pipefail

SETTINGS="$HOME/.claude/settings.json"
PRODUCER="$HOME/.local/bin/simplebar-statusline"
STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
RECORD="$STATE_HOME/simplebar/install.json"
STATE_DIR="$STATE_HOME/simplebar"
CONFIG_DIR="$CONFIG_HOME/simplebar"
INSTALLED_APP="/Applications/SimpleBar.app"
DMG="src-tauri/target/release/bundle/dmg/SimpleBar_0.1.0_x64.dmg"

usage() {
    cat <<'EOF'
Usage: scripts/test-fresh-install.sh <command>

  reset             Make the machine look like SimpleBar has never been
                    installed: removes /Applications/SimpleBar.app, the copied
                    producer, the state directory (reading and setup record),
                    the preferences, and the statusLine entry — backing up
                    settings.json and config.json first. Also quarantines the
                    built DMG so Gatekeeper refuses the first launch, as it
                    would on a real download.

  sandbox           Show the same first-run experience WITHOUT touching
                    anything: runs the built app against a throwaway home
                    directory. Nothing on your machine changes, and clicking
                    Connect writes only into the throwaway. Prefer this unless
                    you specifically want to rehearse Gatekeeper or the drag
                    to Applications.

  restore <backup>  Put a settings.json backup back, exactly as it was.

  status            Show what is currently wired, and change nothing.

After `reset`: open the DMG, drag SimpleBar to Applications, and the Connect
button should be there. Afterwards either `restore` the backup or run
`make install-statusline` to go back to the developer wiring.
EOF
}

# What the app itself would report, worked out the same way — see
# statusline_state() in src-tauri/src/lib.rs.
show_status() {
    echo "settings.json: $SETTINGS"
    if [ ! -f "$SETTINGS" ]; then
        echo "  (absent — reads as not wired)"
    else
        wired=$(node -e '
            const fs = require("fs");
            try {
                const s = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
                process.stdout.write(s?.statusLine?.command ?? "");
            } catch (e) {
                process.stdout.write("<unparseable>");
            }
        ' "$SETTINGS")
        if [ -z "$wired" ]; then
            echo "  statusLine: none — the Connect button will show"
        else
            echo "  statusLine: $wired"
            case "$wired" in
            *simplebar-statusline)
                echo "  → recognised as SimpleBar; the button stays hidden"
                ;;
            *)
                echo "  → another tool; the Connect button will show"
                ;;
            esac
        fi
    fi

    echo
    [ -f "$PRODUCER" ] && echo "producer:      $PRODUCER" || echo "producer:      not installed"
    [ -f "$RECORD" ] && echo "setup record:  $RECORD" || echo "setup record:  not written"
}

reset_install() {
    echo "This edits Claude Code's own config:"
    echo "  $SETTINGS"
    echo
    echo "Your status line will stop working until you click Connect in the app,"
    echo "restore the backup, or run make install-statusline."
    echo
    printf 'Type "yes" to continue: '
    read -r reply
    [ "$reply" = "yes" ] || { echo "Cancelled — nothing changed."; exit 1; }

    if [ -f "$SETTINGS" ]; then
        # Timestamped and in $HOME, not beside the original: the app writes its
        # own settings.json.bak on connect and would overwrite a backup kept
        # there, which is precisely the file you would want during a failed test.
        backup="$HOME/settings.json.before-simplebar-test.$(date +%Y%m%d-%H%M%S)"
        cp "$SETTINGS" "$backup"
        echo "Backed up to $backup"

        # Read-modify-write, dropping only statusLine. Writing a fresh file
        # would discard every other setting in it.
        node -e '
            const fs = require("fs");
            const path = process.argv[1];
            const settings = JSON.parse(fs.readFileSync(path, "utf8"));
            delete settings.statusLine;
            fs.writeFileSync(path, JSON.stringify(settings, null, 2) + "\n");
        ' "$SETTINGS"
        echo "Removed the statusLine entry; every other setting kept."
    else
        echo "No settings.json — already looks fresh."
    fi

    # Preferences too. Left in place, the app would open at your remembered
    # size and position with your mute and pin settings — a returning user's
    # window, not a new one's.
    if [ -f "$CONFIG_DIR/config.json" ]; then
        cp "$CONFIG_DIR/config.json" "$HOME/simplebar-config.json.before-test.$(date +%Y%m%d-%H%M%S)"
        echo "Backed up preferences to \$HOME"
    fi
    rm -rf "$CONFIG_DIR" && echo "Removed preferences ($CONFIG_DIR)"

    # The whole state directory, not just the setup record: a leftover
    # usage.json would show a live number where a new user sees "no data yet".
    rm -rf "$STATE_DIR" && echo "Removed state ($STATE_DIR)"
    rm -f "$PRODUCER" && echo "Removed $PRODUCER"

    # Quitting first: a running instance rewrites its config on exit, which
    # would put back the geometry just deleted.
    osascript -e 'quit app "SimpleBar"' >/dev/null 2>&1 || true
    sleep 1
    if [ -d "$INSTALLED_APP" ]; then
        rm -rf "$INSTALLED_APP"
        echo "Removed $INSTALLED_APP"
    fi

    # Gatekeeper only fires on a file carrying the quarantine flag, which is
    # set by whatever downloaded it. A locally built DMG has none, so the step
    # that stops every real user is the one step a local test would skip.
    if [ -f "$DMG" ]; then
        xattr -w com.apple.quarantine "0081;00000000;test;" "$DMG" 2>/dev/null \
            && echo "Quarantined the DMG — Gatekeeper will refuse the first launch"
    else
        echo "No DMG at $DMG — run make build first"
    fi

    echo
    echo "Ready. Open the DMG, drag SimpleBar across, and launch it."
    echo "Expect: a Gatekeeper refusal, then after approving it, a default-sized"
    echo "window reading \"no data yet\" with a Connect to Claude Code button."
}

# The same first run, with nothing at stake: a throwaway HOME means the app
# finds no settings, no preferences and no reading, so it behaves exactly as it
# would on a new machine — and a Connect click writes into the throwaway rather
# than the real one. This is how the button was first verified.
run_sandbox() {
    app="src-tauri/target/release/bundle/macos/SimpleBar.app/Contents/MacOS/simplebar"
    [ -x "$app" ] || { echo "No built app at $app — run make build first."; exit 1; }

    sandbox=$(mktemp -d "${TMPDIR:-/tmp}/simplebar-sandbox.XXXXXX")
    echo "Throwaway home: $sandbox"
    echo "Nothing outside it will be touched. Close the window when finished."
    echo

    HOME="$sandbox" \
    XDG_STATE_HOME="$sandbox/.local/state" \
    XDG_CONFIG_HOME="$sandbox/.config" \
        "$app" >/dev/null 2>&1 || true

    echo "Window closed. What the sandbox ended up holding:"
    find "$sandbox" -type f 2>/dev/null | sed "s|$sandbox|~|" || true
    rm -rf "$sandbox"
}

restore_backup() {
    backup="${1:-}"
    [ -n "$backup" ] || { echo "restore needs a backup file. Try: ls ~/settings.json.before-simplebar-test.*"; exit 1; }
    [ -f "$backup" ] || { echo "No such file: $backup"; exit 1; }

    # Parse before overwriting: restoring a truncated backup over a working
    # config would be a worse outcome than refusing.
    node -e 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))' "$backup" \
        || { echo "$backup is not valid JSON — refusing to restore it."; exit 1; }

    cp "$backup" "$SETTINGS"
    echo "Restored $SETTINGS from $backup"
}

case "${1:-}" in
reset) reset_install ;;
sandbox) run_sandbox ;;
restore) restore_backup "${2:-}" ;;
status) show_status ;;
*) usage; exit 1 ;;
esac
