# SimpleBar

**A visual usage monitor for Claude Code.** See how much of your Claude
5-hour session limit is left, without asking.

SimpleBar is a macOS HUD — an ordinary resizable window, not a WidgetKit
widget — that draws your remaining session limit as a circular gauge and
drains it as you use it. The number is account-wide: it already includes
claude.ai and mobile usage, not just this machine.

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

**M1–M4, M6, M7 and M9 built.** The producer, window, file watch and wheel
work: the wheel drains, changes colour at 20% and 10%, and follows the reading
live while Claude Code runs. It beeps once on each threshold crossing with a
mute toggle (M6), shows no-data and stale states (M7), and remembers its size
and position across restarts (M9). What remains is packaging into a `.app`
bundle (M8). See `DESIGN.md` for the full milestone list — including M5, an
adjustable-translucency menu, dropped by decision.

## Requirements

- **macOS.** Stage 1 is macOS only.
- **Rust** — install via [rustup](https://rust-lang.org/tools/install/)
  ([other methods](https://forge.rust-lang.org/infra/other-installation-methods.html#which)).
- **Node.js** (18 or newer) for the Tauri CLI.
- **Xcode Command Line Tools** — Tauri builds the window against them. If you
  don't have them: `xcode-select --install`.

## Build and run

There's no packaged `.app` yet (that's M8) — for now you run it from source.

```sh
git clone https://github.com/daniel-p-allen/SimpleBar.git
cd SimpleBar

cargo build --manifest-path statusline/Cargo.toml   # the producer
npm install && npx tauri dev                         # the window
```

The first `npx tauri dev` compiles the Rust window, so it takes a few minutes;
later runs are fast. The window opens showing "no data yet" until you wire the
producer into Claude Code (below) and run a prompt.

Check your work:

```sh
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
```

## License

MIT — see `LICENSE`.
