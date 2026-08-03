#!/bin/sh
# Stop everything `npx tauri dev` started.
#
# Exists because stopping it by hand reliably leaves orphans. `tauri dev` runs
# four processes deep — a wrapper shell, `npm exec tauri`, the node CLI, and
# the compiled binary — so a pattern that matches the binary kills the app and
# leaves the supervisor alive, and one that matches node leaves the shell.
# Orphans are quiet: they hold no window, cost no CPU, and then confuse the
# next `pgrep` into reporting instances that are not running.
#
# Kills outside-in so the supervisor cannot restart the binary as it dies.

set -eu

killed=0

stop() {
  pattern="$1"
  label="$2"

  pids=$(pgrep -f "$pattern" 2>/dev/null || true)
  [ -z "$pids" ] && return 0

  for pid in $pids; do
    # Never kill this script, or the shell running it.
    [ "$pid" = "$$" ] && continue
    [ "$pid" = "$PPID" ] && continue

    kill "$pid" 2>/dev/null || continue
    echo "  stopped $label (pid $pid)"
    killed=$((killed + 1))
  done
}

stop "npx tauri dev"        "tauri dev wrapper"
stop "npm exec tauri"       "npm exec"
stop "node .*bin/tauri"     "tauri CLI"
stop "target/debug/simplebar" "simplebar"

# A moment for the supervisors to go, then sweep anything that outlived them.
sleep 1
stop "target/debug/simplebar" "simplebar (orphaned)"

if [ "$killed" -eq 0 ]; then
  echo "dev-stop: nothing running."
else
  echo "dev-stop: stopped $killed process(es)."
fi
