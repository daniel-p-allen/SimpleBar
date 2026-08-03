# SimpleBar

See how much of your Claude 5-hour session limit is left, without asking.

SimpleBar is a macOS HUD — an ordinary resizable window, not a WidgetKit
widget — that draws your remaining session limit as a truck-tyre wheel and
drains it as you use it. The number is account-wide: it already includes claude.ai and mobile
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

**M4 done.** The producer, the window, the file watch and the wheel all work:
the wheel drains, changes colour at 20% and 10%, and follows the reading live
while Claude Code runs. Still to come are the alert beeps (M6), the no-data
and stale states (M7), and packaging (M8). See `DESIGN.md` for the milestone
list — including M5, an adjustable-translucency menu, dropped by decision.

## Build and try it

Both halves are Rust; the window is Tauri, so it also needs Node for the
Tauri CLI.

```sh
cd statusline && cargo build      # the producer
npm install && npx tauri dev      # the window

make test    # both test suites
make check   # refuse to ship a committed credential
```

Feed the producer a status-line blob by hand:

```sh
cat tests/fixtures/full.json | statusline/target/debug/simplebar-statusline
# 35% left · resets 1:00 am
```

## Wire it into Claude Code

Add to `~/.claude/settings.json` (back up first — this repo doesn't yet ship
an installer that merges safely with an existing status line):

```json
"statusLine": {
  "type": "command",
  "command": "/path/to/SimpleBar/statusline/target/debug/simplebar-statusline"
}
```

The reading is written to `$XDG_STATE_HOME/simplebar/usage.json` (default
`~/.local/state/simplebar/`), per the XDG Base Directory Specification.

## Layout

```
statusline/    Rust — the simplebar-statusline producer, and its tests
src-tauri/     Rust — window, file watching
src/           webview UI — HTML/CSS/SVG, the wheel
scripts/       repo tooling (check-secrets.sh)
tests/         shared fixtures, and the consumer's tests
docs/
```

## License

MIT — see `LICENSE`.
