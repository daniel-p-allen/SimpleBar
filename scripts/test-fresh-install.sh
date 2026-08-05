#!/usr/bin/env bash
#
# Make this machine look like it has never installed SimpleBar, so the
# Connect button can be tested the way a new user meets it — and put it back
# afterwards.
#
# Why it exists: a developer's machine is wired by `make install-statusline`,
# and that deliberately counts as connected, so the button never appears. The
# only way to see what a stranger sees is to remove the three things that say
# "already installed":
#
#   * the copied producer at ~/.local/bin/simplebar-statusline
#   * the setup record at $XDG_STATE_HOME/simplebar/install.json
#   * the statusLine entry in ~/.claude/settings.json
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
RECORD="$STATE_HOME/simplebar/install.json"

usage() {
    cat <<'EOF'
Usage: scripts/test-fresh-install.sh <command>

  reset             Make the machine look freshly installed. Backs up
                    settings.json first, then removes the statusLine entry,
                    the copied producer, and the setup record.

  restore <backup>  Put a settings.json backup back, exactly as it was.

  status            Show what is currently wired, and change nothing.

After `reset`: open SimpleBar from the DMG and the Connect button should be
there. After testing, either `restore` the backup or run
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

    rm -f "$PRODUCER" && echo "Removed $PRODUCER"
    rm -f "$RECORD" && echo "Removed $RECORD"

    echo
    echo "Ready. Open SimpleBar from the DMG — Connect to Claude Code should appear."
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
restore) restore_backup "${2:-}" ;;
status) show_status ;;
*) usage; exit 1 ;;
esac
