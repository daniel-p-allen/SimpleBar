# SimpleBar

See how much of your Claude 5-hour session limit is left, without asking.

SimpleBar is a macOS HUD — an 800×800 window, not a WidgetKit widget — that
draws your remaining session limit as a truck-tyre wheel and drains it as you
use it. The number is account-wide: it already includes claude.ai and mobile
usage, not just this machine.

```
Claude Code ──stdin JSON──▶ simplebar-statusline ──▶ usage.json
                                                          │ file watch
                                                          ▼
                                                  SimpleBar (Tauri)
```

No credentials, no network calls. Claude Code's `statusLine` feature already
hands a status-line command the numbers it needs
(`rate_limits.five_hour.used_percentage`, `resets_at`) on stdin — SimpleBar
just writes them to a file and watches it. See `DESIGN.md` for the full
reasoning, including why the OAuth-endpoint alternative was deliberately
deferred.

## Status

**M1 in progress.** Only the producer exists — a script that turns Claude
Code's status-line JSON into `35% left · resets 1:00 am` and writes the
reading to disk. The window, the wheel, and everything visual are M2 onward
and are not built yet. See `DESIGN.md` for the milestone list.

## Try the producer

No build needed yet — it's dependency-free Python, standing in for the Rust
binary that M1 will end with.

```sh
make test    # run it against fixtures, including four ways it should fail quietly
make check   # refuse to ship a committed credential
```

Feed it a real status-line blob by hand:

```sh
cat tests/fixtures/full.json | python3 scripts/statusline-prototype.py
# 35% left · resets 1:00 am
```

## Wire it into Claude Code

Add to `~/.claude/settings.json` (back up first — this repo doesn't yet ship
an installer that merges safely with an existing status line):

```json
"statusLine": {
  "type": "command",
  "command": "python3 /path/to/SimpleBar/scripts/statusline-prototype.py"
}
```

The reading is written to `$XDG_STATE_HOME/simplebar/usage.json` (default
`~/.local/state/simplebar/`), per the XDG Base Directory Specification.

## Layout

```
src-tauri/     Rust — window, file watching, atomic writes (M2+)
src/           webview UI — HTML/CSS/SVG, the wheel and alert logic (M2+)
scripts/       the producer, plus repo tooling (check-secrets.sh)
tests/         fixtures and tests for the producer
docs/
```

## License

MIT — see `LICENSE`.
