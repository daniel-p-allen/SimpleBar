# CLAUDE.md

Project guide for Claude Code. `DESIGN.md` is the source of truth for every
design decision — if the code and the doc disagree, update the doc first, then
fix the code. No silent drift.

M1–M4, M6, M7 and M9 are done: the producer, the window, the file watch, the
wheel, the beeps and mute toggle, the stale and no-data states, and the
remembered window geometry all work. M5 (adjustable translucency) was dropped
by decision — see "Decisions changed" in `DESIGN.md`. M8 (packaging) is the
next and last piece of code, scoped to an unsigned local build — no Apple
Developer account, so no signing, notarization or clean-Mac distribution.

## What this is

SimpleBar is a macOS **HUD** — an ordinary resizable app window, opening at
800×800, that shows how much of the Claude 5-hour session limit is left, as a
circular gauge that drains as you use it.

It is **not** a widget. macOS "widget" means WidgetKit, which can't be
alt-tabbed, can't beep, and won't refresh on demand. Don't drift back toward
it.

## Layout

```
src-tauri/     Rust — window, file watching, atomic writes
src/           webview UI — HTML/CSS/SVG, the wheel and alert logic
statusline/    the simplebar-statusline producer binary, and its tests
tests/         shared fixtures, and the consumer's tests
scripts/       repo tooling, including check-secrets.sh
```

## Commands

`make` is the single entry point, as in the other repos. `test`, `check`,
`clean` and `dev-stop` exist; `build`, `run` and `install-statusline` are the
agreed contract, arriving with M8 packaging, not yet written.

```bash
make test               # unit tests, both sides            (exists)
make check              # refuse to ship a committed credential  (exists)
make clean                                                  # (exists)
make dev-stop           # stop the dev app and everything it started (exists)
make build              # build both binaries               (M8)
make run                # run the app against the current usage.json (M8)
make install-statusline # wire the producer into ~/.claude/settings.json (M8)
```

Until `make build` and `make run` exist, the app runs via `npx tauri dev` and
the producer via `cargo build --manifest-path statusline/Cargo.toml`. Note
that `cargo` is not on the PATH of a non-interactive shell here — export
`~/.cargo/bin` first.

Comment the Makefile the way the other repos do — say *why* a target exists,
not just what it runs.

## How the data flows

```
Claude Code ──stdin JSON──▶ simplebar-statusline ──▶ usage.json
                                                        │ file watch
                                                        ▼
                                                 SimpleBar (Tauri)
```

Files follow the XDG Base Directory Specification — the reading lives at
`$XDG_STATE_HOME/simplebar/usage.json` (default `~/.local/state/simplebar/`),
preferences at `$XDG_CONFIG_HOME/simplebar/config.json`. Never write a
dotfolder into `$HOME`. `DESIGN.md` has the full table and the standards we
follow.

Claude Code's `statusLine` command receives a JSON blob on stdin containing
`rate_limits.five_hour.used_percentage` and `resets_at`. Those are
account-wide figures — they already include claude.ai and mobile usage.

## Rules

- **No credentials, ever.** Stage 1 reads a local file and makes no network
  calls. Don't add a keychain read or an API call without an explicit decision
  recorded in `DESIGN.md` — that route is described there and deliberately
  deferred.
- **Never print, log or echo a token.** Applies to debugging output too.
- **The producer must never panic and never write garbage.** `rate_limits` is
  optional and the schema is Claude Code's internal contract. On anything
  unexpected, leave the last good `usage.json` in place and exit quietly.
- **Writes are atomic** — temp file plus rename, so the watcher never reads a
  half-written file.
- **Never show a stale number as if it were live.** The reading only refreshes
  while Claude Code is running. `written_at` exists so the UI can say "as of
  14:02".
- **Beeps fire once per threshold crossing**, not once per update.
- **Stage 1 is macOS only.** No server, no mobile, no tray icon, no
  always-on-top. Those are later stages and are out of scope.

## Testing

Test the parsing and the state logic, not the pixels. Producer: fixture blob
in, expected `usage.json` out — including `rate_limits` absent, malformed JSON
and empty stdin. Consumer: colour bands (including exactly 20 and 10),
once-only threshold crossing, epoch → local clock string, staleness, no-data.

The SVG and the translucency are checked by eye at milestone checkpoints.

## Working style

- Small steps. One milestone from `DESIGN.md` at a time — each ships and is
  verifiable on its own. Don't bundle two.
- Integration checkpoints are at M2, M4 and M8. Stop and confirm it works in
  the real app before moving on.
- **Work in short batches and stop.** A handful of related file changes, then
  hand back — don't disappear into a long unbroken run. Dan interjects mid-task
  often, and a shorter turn means his correction lands before the work built on
  the wrong assumption, not after.
