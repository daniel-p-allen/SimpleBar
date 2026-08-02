#!/usr/bin/env python3
"""Tests for the SimpleBar producer.

The producer runs inside Claude Code's status line, so the interesting cases
are the bad ones. Every malformed input must leave the last good usage.json
alone, print nothing, and exit 0 — a status line that errors is worse than one
that is blank.

No test framework: plain asserts and a subprocess, so `make test` needs nothing
installed.
"""

import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PRODUCER = os.path.join(ROOT, "scripts", "statusline-prototype.py")
FIXTURES = os.path.join(ROOT, "tests", "fixtures")

failures = []


def run(stdin_bytes, state_home):
    """Run the producer with its state directory redirected, return (out, code)."""
    env = dict(os.environ, XDG_STATE_HOME=state_home)
    proc = subprocess.run(
        [sys.executable, PRODUCER],
        input=stdin_bytes,
        capture_output=True,
        env=env,
    )
    return proc.stdout.decode(), proc.stderr.decode(), proc.returncode


def fixture(name):
    with open(os.path.join(FIXTURES, name), "rb") as fh:
        return fh.read()


def usage_path(state_home):
    return os.path.join(state_home, "simplebar", "usage.json")


def check(label, condition, detail=""):
    if condition:
        print(f"  ok   {label}")
    else:
        print(f"  FAIL {label} {detail}")
        failures.append(label)


def test_full_blob():
    with tempfile.TemporaryDirectory() as tmp:
        out, err, code = run(fixture("full.json"), tmp)
        check("full blob: exits 0", code == 0, f"(got {code})")
        check("full blob: nothing on stderr", err == "", f"(got {err!r})")
        check("full blob: reports remaining, not used", "35%" in out, f"(got {out!r})")
        check("full blob: 12-hour reset time", "am" in out or "pm" in out, f"(got {out!r})")

        with open(usage_path(tmp)) as fh:
            data = json.load(fh)
        check("full blob: writes used_percentage", data["used_percentage"] == 65)
        check("full blob: writes resets_at", data["resets_at"] == 1785682800)
        check("full blob: stamps written_at", isinstance(data["written_at"], int))
        check("full blob: ignores seven_day", "seven_day" not in data)


def test_quiet_on_bad_input():
    """Every one of these must be silent, exit 0, and write nothing."""
    cases = [
        ("rate_limits absent", fixture("no_rate_limits.json")),
        ("five_hour absent", fixture("five_hour_missing.json")),
        ("malformed JSON", fixture("malformed.json")),
        ("empty stdin", b""),
        ("not JSON at all", b"hello"),
        ("JSON but not an object", b"[1, 2, 3]"),
        ("used_percentage is a string", b'{"rate_limits":{"five_hour":'
                                        b'{"used_percentage":"65","resets_at":1}}}'),
    ]
    for label, payload in cases:
        with tempfile.TemporaryDirectory() as tmp:
            out, err, code = run(payload, tmp)
            check(f"{label}: exits 0", code == 0, f"(got {code})")
            check(f"{label}: prints nothing", out.strip() == "", f"(got {out!r})")
            check(f"{label}: nothing on stderr", err == "", f"(got {err!r})")
            check(f"{label}: writes no file", not os.path.exists(usage_path(tmp)))


def test_bad_input_preserves_last_good_reading():
    """The one that matters: a bad blob must not destroy a good reading."""
    with tempfile.TemporaryDirectory() as tmp:
        run(fixture("full.json"), tmp)
        before = open(usage_path(tmp)).read()

        for payload in (fixture("malformed.json"), b"", fixture("no_rate_limits.json")):
            run(payload, tmp)

        after = open(usage_path(tmp)).read()
        check("bad input leaves the last good reading untouched", before == after)


def test_no_temp_file_left_behind():
    """Atomic write means temp + rename; the temp must not survive."""
    with tempfile.TemporaryDirectory() as tmp:
        run(fixture("full.json"), tmp)
        leftovers = [
            f for f in os.listdir(os.path.join(tmp, "simplebar")) if f.endswith(".tmp")
        ]
        check("no .tmp file left behind", leftovers == [], f"(found {leftovers})")


def main():
    for test in (
        test_full_blob,
        test_quiet_on_bad_input,
        test_bad_input_preserves_last_good_reading,
        test_no_temp_file_left_behind,
    ):
        print(test.__name__)
        test()

    print()
    if failures:
        print(f"{len(failures)} failure(s): {', '.join(failures)}")
        sys.exit(1)
    print("all passed")


if __name__ == "__main__":
    main()
