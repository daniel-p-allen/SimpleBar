#!/usr/bin/env python3
"""Prototype of the SimpleBar producer — M1, with Python standing in for Rust.

Claude Code pipes its status line JSON blob to this script on stdin. We pull out
the 5-hour subscription window, write it where SimpleBar will watch for it, and
print a one-line status.

Deliberately silent on failure: `rate_limits` is absent for non-subscribers and
before the first API response of a session, and the schema is Claude Code's
internal contract. A status line that crashes or prints a stack trace is worse
than one that prints nothing, so anything unexpected leaves the last good
usage.json alone and exits quietly.

Follows the CLI Guidelines (https://clig.dev/): the status line text is the
primary output and goes to stdout, nothing is written to stderr, and we exit 0.
The deliberate deviation is that we exit 0 on bad input too — a status line is
not a normal CLI consumer, and a non-zero exit here would be noise nobody acts
on.
"""

import json
import os
import sys
import time

def _state_dir() -> str:
    """`$XDG_STATE_HOME`, defaulting to ~/.local/state.

    SimpleBar is both a CLI (this producer) and a GUI (the Tauri app). macOS
    convention would put GUI data under ~/Library/Application Support, but the
    two halves share this one file and the producer is the half that runs in a
    terminal — so we follow XDG, which also makes a Linux port free later.
    """
    return os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state")


OUT_DIR = os.path.join(_state_dir(), "simplebar")
OUT_FILE = os.path.join(OUT_DIR, "usage.json")


def main() -> None:
    try:
        blob = json.load(sys.stdin)
    except Exception:
        return

    # Valid JSON is not necessarily an object — a bare list or string parses
    # fine and would then blow up on .get().
    if not isinstance(blob, dict):
        return

    rate_limits = blob.get("rate_limits")
    if not isinstance(rate_limits, dict):
        return

    five_hour = rate_limits.get("five_hour")
    if not isinstance(five_hour, dict):
        return

    used = five_hour.get("used_percentage")
    resets_at = five_hour.get("resets_at")

    if not isinstance(used, (int, float)) or not isinstance(resets_at, (int, float)):
        # Nothing to report yet. Leave any existing reading in place.
        return

    remaining = 100 - used
    # 12-hour with am/pm — "1:00 am", not "01:00". No leading zero on the hour.
    resets = time.strftime("%-I:%M %p", time.localtime(resets_at)).lower()

    # Purely cosmetic — a small label under the reset time in the app. Never
    # blocks the required fields above; absent or malformed model info just
    # means no label.
    model = blob.get("model")
    model_name = model.get("display_name") if isinstance(model, dict) else None
    if not isinstance(model_name, str):
        model_name = None

    # Atomic write — temp file plus rename — so a reader never sees a
    # half-written file.
    try:
        os.makedirs(OUT_DIR, exist_ok=True)
        tmp = OUT_FILE + ".tmp"
        with open(tmp, "w") as fh:
            json.dump(
                {
                    "used_percentage": used,
                    "resets_at": resets_at,
                    "written_at": int(time.time()),
                    "model_name": model_name,
                },
                fh,
            )
        os.replace(tmp, OUT_FILE)
    except Exception:
        pass  # Never let a write failure take out the status line.

    print(f"{remaining:.0f}% left · resets {resets}")


if __name__ == "__main__":
    main()
