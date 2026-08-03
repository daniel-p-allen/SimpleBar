# SimpleBar — Design

A single-window macOS app that shows how much of your Claude 5-hour session
limit is left, as a truck-tyre wheel that drains as you use it.

Status: agreed, stage 1 (macOS) not yet built.

## What it is

Not a widget. macOS "widget" means WidgetKit — lives in Notification Centre,
sits behind your windows, can't be alt-tabbed, refreshes on the system's
schedule, can't play a sound, must be Swift. Every one of those collides with
the requirement. SimpleBar is a **HUD**: an ordinary application window whose
whole job is to display one live number. It opens at 800×800.

- Resizable, down to a 300×300 floor. Everything scales with the window —
  the wheel tracks the *smaller* dimension, so it stays circular rather than
  stretching into an ellipse when the window is dragged out of square, and
  the centre text scales with it rather than staying put and overflowing.
  The floor exists because below roughly 300 the reading stops being
  readable, and a HUD that can be dragged down to an illegible smudge is a
  worse default than one that refuses.
- On screen, coverable by other windows, alt-tabbable. Not always-on-top.
- Fixed translucency: the window background is ~95% see-through, set in the
  stylesheet. Not adjustable — see "Adjustable translucency" under decisions
  changed.
- Wheel starts full, drains clockwise as the session is consumed.
- Amber at 20% remaining or below, red at 10% or below (inclusive — exactly
  10% is red, not amber), one beep per threshold crossing.
- Centre shows the percentage remaining, then the reset time on a 12-hour
  clock, lower case, no leading zero on the hour: "97% — resets 1:00 am",
  "9% — resets 10:00 pm". Below it, in a smaller font, the model active when
  the reading was taken, with its effort level: "Opus 5 · Medium". The effort
  is title-cased from the blob's lower case, and separated by a middle dot
  with thin spaces — lighter than the em dash above, because the line is
  subordinate to the reading and effort qualifies the model rather than
  standing beside it. Falls back to the bare model name when effort is
  absent, and the line is omitted entirely when the model is absent too,
  rather than showing a blank line.

  The percentage is not decoration: it is the non-colour cue required by the
  accessibility rule below, so the amber and red states stay legible to
  someone who cannot separate them by hue.

## Where the number comes from

Claude Code's `statusLine` feature pipes a JSON blob to a command you nominate
in `~/.claude/settings.json`, on every status line render. That blob carries:

```
"rate_limits": {          // Claude.ai subscription usage limits
  "five_hour": { "used_percentage": 0-100, "resets_at": <unix epoch seconds> },
  "seven_day": { "used_percentage": 0-100, "resets_at": <unix epoch seconds> }
}
```

SimpleBar uses `five_hour` only. Separately, the blob also carries two
top-level fields used for the small label under the reset time, and for no
usage calculation:

```
"model":  { "display_name": "Opus 5" },
"effort": { "level": "medium" }        // lower case in the blob
```

The blob carries considerably more than this — `context_window`, `cost`,
`thinking`, `fast_mode`, `output_style`, `version`, `transcript_path`. None of
it is read, and the fields above are the whole of SimpleBar's dependency on
Claude Code's internal schema. `context_window` in particular is *not* the
session limit and must not be confused with `rate_limits.five_hour`.

These are **account-wide** figures — the same allowance drawn down by
claude.ai in the browser and the mobile apps — so the number is correct even
though it arrives via Claude Code.

**No credentials are involved.** Nothing is sent anywhere, nothing is read
from the keychain, and `usage.json` holds two percentages and two timestamps.

### Rejected alternatives

- **Parsing `~/.claude/projects/**/*.jsonl`.** Reconstructs usage from local
  transcripts. Only sees Claude Code on this machine, and infers the limit
  rather than reading it. Superseded.
- **`GET /api/oauth/usage` on `api.anthropic.com`** with the OAuth token from
  the login keychain (service `Claude Code-credentials`). This is the only way
  to refresh while Claude Code *isn't* running, and the only way to feed a
  phone. Rejected for stage 1: undocumented, can change without notice,
  requires handling a live token, and sits in a grey area of Anthropic's terms.
  Kept behind the `UsageSource` boundary so it can be added later as a
  deliberate decision, not a default.

## Architecture

Two binaries, one file between them.

```
Claude Code ──stdin JSON──▶ simplebar-statusline ──▶ usage.json
                                                          │
                                                    file watch
                                                          ▼
                                                  SimpleBar (Tauri)
```

### File locations

Per the XDG Base Directory Specification, not a dotfolder in `$HOME`:

| What | Where |
|---|---|
| The reading | `$XDG_STATE_HOME/simplebar/usage.json`, default `~/.local/state/simplebar/` |
| Preferences (mute) | `$XDG_CONFIG_HOME/simplebar/config.json`, default `~/.config/simplebar/` |
| Installed producer binary | `~/.local/bin/simplebar-statusline` |
| Setup record | `$XDG_STATE_HOME/simplebar/install.json` |

SimpleBar is both a CLI (the producer) and a GUI (the app). macOS convention
would put GUI data under `~/Library/Application Support`, but the two halves
share these files and the producer is the half that runs in a terminal — so XDG
wins, and a Linux port comes free.

**`simplebar-statusline`** — a small Rust binary registered as the statusLine
command. Reads the JSON blob on stdin, extracts `five_hour` and (if present)
`model.display_name`, writes `usage.json` atomically (temp file + rename, so
the reader never sees a half-written file), prints the status line text. Rust
rather than shell+`jq` so there is no runtime dependency to install.

**SimpleBar** — a Tauri app. Rust side owns the window and watches `usage.json`
with the `notify` crate, emitting an event to the webview on change. The
webview draws the wheel as an SVG and owns the alert logic.

The watch is on the *directory*, not on `usage.json` itself. Because the
producer writes atomically — temp file plus rename — each write replaces the
inode, and a file watch would follow the old one and stop firing after the
first update. Directory events are filtered by filename.

`usage.json` schema:

```json
{
  "used_percentage": 62.5,
  "resets_at": 1754150400,
  "written_at": 1754140812,
  "model_name": "Opus 5",
  "effort_level": "medium"
}
```

`model_name` and `effort_level` are nullable — written as `null` when the blob
had no `model` or `effort` field, never omitted, so readers don't have to
distinguish "absent key" from "no value". `effort_level` is stored as the blob
gives it, lower case; casing for display is the consumer's business.

Readers must tolerate both fields being missing from the file altogether: a
`usage.json` written by an older producer predates them, and failing to parse
it would throw away a perfectly good reading.

`written_at` is ours, not Claude Code's — it is what lets the UI say how old
the reading is.

### Stack

**Tauri** (Rust + system webview). ~5MB binary, cross-platform when stages 2+
arrive, and the Rust surface here is real but small — window setup, file
watching, atomic writes. Chosen partly for the Rust experience.

Rejected: **Electron** (~200MB for one SVG), **Flutter** (new language, no
Rust), **React Native** (mobile-first, no Linux target), **Swift/SwiftUI**
(Apple-only, dead end for later stages).

### Boundaries

`UsageSource` is the seam: something that yields `{used_percentage, resets_at,
written_at}`. Stage 1 has exactly one implementation, the file watcher. The
UI never learns where the number came from, so adding the endpoint or a synced
file later touches one module.

## Failure isolation

The pieces share one file and nothing else. No piece is allowed to take another
down with it.

- **The producer must never damage Claude Code.** It runs inside the status
  line, in the critical path of every prompt render. It exits 0 on every input,
  prints nothing rather than an error, writes nothing to stderr, and never
  blocks — anything it shells out to gets a hard timeout. Worst case is a blank
  status line.
- **A dead producer does not break the app.** No new write simply means the
  reading ages, and the app already renders age from `written_at`. It degrades
  to "as of 2:14 pm", not to a crash or a stale number pretending to be live.
- **A dead app does not break the producer.** The producer neither knows nor
  cares whether SimpleBar is running.
- **A corrupt or missing `usage.json` does not break the app.** Parse failure is
  treated as no-data — grey wheel, "no data yet". The last good reading is kept
  in memory and marked stale rather than thrown away.
- **A chained third-party status line failing does not break ours.** Their
  command runs with a timeout; on non-zero exit, timeout or crash we print our
  segment alone and carry on.
- **The settings edit is all-or-nothing.** Back up, parse, modify in memory,
  write a temp file, re-parse it to prove it is valid, then rename. Any failure
  leaves the original untouched.
- **Nothing in stage 1 touches the network,** so there is no outage to handle.

## Constraints and known limits

- **Freshness, not coverage.** The figures include phone and browser usage,
  but only refresh while Claude Code is running. An afternoon on your phone
  leaves the reading stale until you next open a session. The UI must show
  "as of 14:02" so it never silently lies.
- **`rate_limits` is optional.** The field is absent for non-subscribers and
  before the first API response of a session. The producer writes nothing
  rather than writing zeros.
- **The schema is Claude Code's internal contract** and can change without
  notice. The producer tolerates missing or renamed fields, never panics, and
  leaves the last good file in place.
- **Beeps arrive late or bunched** after a gap in Claude Code activity, for the
  same reason as staleness.
- Stage 1 is macOS only. No server, no mobile, no tray icon.

## Standards adopted

Four, chosen because each one closes a specific weakness rather than because
they are box-ticking.

- **[XDG Base Directory Specification](https://specifications.freedesktop.org/basedir/latest/)**
  — file locations, as above. Adopted now, because paths are painful to change
  once people have installed.
- **[Command Line Interface Guidelines](https://clig.dev/)** — governs the
  producer. Status line text is the primary output and goes to stdout; nothing
  goes to stderr; exit 0 on success; exit codes ≥126 are avoided because they
  collide with shell codes. **Deliberate deviation:** we exit 0 on bad input
  too. A status line is not a normal CLI consumer and a non-zero exit would be
  noise nobody acts on.
- **[WCAG 2.2, 1.4.1 Use of Color](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)**
  — applies at M4. The 20% and 10% states must not be signalled by colour
  alone; amber→red is exactly the red-green axis that around 8% of men cannot
  distinguish. Pair the colour change with a second cue (centre text, or a
  change in the tread).
- **[Semantic Versioning](https://semver.org/)** and
  **[Keep a Changelog](https://keepachangelog.com/)** — from the first release,
  so the version in `install.json` supports a real comparison when deciding
  whether an installed producer is stale.

## Performance

Modest, but worth stating so we notice regressions:

- Idle CPU ~0%. The app is event-driven off the file watch; no polling loop
  beyond a 60-second timer that only refreshes the staleness display.
- Memory under ~150MB (dominated by the system webview).
- Cold start to first paint under 1 second.
- Wheel animates to a new value over ~400ms; no continuous animation at rest.

## Testing

What is worth testing here is the parsing and the state logic, not the pixels.

**Producer (Rust unit tests)** — fixture blobs in, expected `usage.json` out:
full blob; `rate_limits` absent; `five_hour` absent but `seven_day` present;
malformed JSON; empty stdin. Plus: the write is atomic, and a bad input leaves
any existing file untouched.

**Consumer (unit tests, pure functions)**:
- percentage → colour band, including exactly on 20 and 10
- threshold crossing fires **once**, not on every update, and re-arms only
  after the window resets
- `resets_at` epoch → 12-hour local clock string ("1:00 am", "10:00 pm"),
  including midnight, noon, and across a day boundary
- staleness from `written_at`
- no-data state

**Not unit tested**: the SVG itself and window translucency — checked by eye at
the milestone checkpoints.

**Integration**: pipe a fixture into the producer and confirm the running app
updates. Manual, at checkpoints.

## Milestones

Each ships and is verifiable on its own.

| # | Deliverable | Done when |
|---|---|---|
| M1 | `simplebar-statusline` binary | `cat fixture.json \| simplebar-statusline` writes correct `usage.json`; unit tests pass |
| M2 | Tauri window reading `usage.json` once at startup, as plain text | Numbers appear on screen — **integration checkpoint: end-to-end wiring proven** |
| M3 | File watching | Editing `usage.json` updates the window live |
| M4 | The wheel — SVG, drains clockwise, colour bands | Looks right at 100/50/19/9% — **integration checkpoint: visual complete** |
| ~~M5~~ | ~~Right-click menu — translucency 20/40/60/80/100~~ | **Dropped, 2026-08-03 — see "Adjustable translucency" below.** Numbering left alone so M6–M8 keep the numbers they were built and discussed under |
| M6 | Alerts — beep on crossing, mute toggle | Fires once per crossing, not per update |
| M7 | No-data and stale states | Grey wheel reading "no data yet" before the first write |
| M8 | Packaging — `.app` bundle, README, statusLine wiring instructions | Installs on a clean account — **integration checkpoint: real use** |

Stage 2 (a sync or server so the number refreshes without Claude Code running)
and stage 3 (mobile) are deliberately out of scope and not designed here.

## Decisions changed

Recorded rather than edited away, so the reasoning behind a reversal is
visible to whoever reads this next.

- **Adjustable translucency — dropped, 2026-08-03.** Originally M5: a
  right-click menu offering 20/40/60/80/100%, with the choice persisted to
  `config.json`. Dropped on review: the fixed ~95%-transparent background
  already reads well against any desktop, so the feature bought a preferences
  file, a menu, and a persistence path for a setting nobody had wanted to
  change. The two open design questions it raised — whether the percentages
  meant opacity or transparency, and whether fading the whole HUD would
  undermine the contrast rules below — both disappear with it.

  The window stays translucent; only the *adjustability* is gone.
  `config.json` survives as the home for the M6 mute toggle, now its only
  setting.

## Ideas parked for later

Not designed, not committed to — captured so they aren't lost, to be properly
designed if and when picked up.

- **A model-switch nudge.** At some usage threshold (40% suggested), surface a
  prompt in the foreground: usage is climbing, switch to a cheaper model to
  make the session stretch further? Would need a defined workflow for actually
  switching — `claude`'s model selection is presently a manual `/model` or
  `--model` action, so this would mean either shelling out to change the
  session default or walking the user through it. Overlaps with the beep/alert
  machinery from M6 but is a distinct feature, not a rename of it.

## Repo conventions

Follows the existing repos: `Makefile` as the single entry point, `scripts/`
for repo tooling, `docs/`, `.editorconfig`, `LICENSE`, `README.md`, GitHub
Actions CI.

Targets: `build`, `run`, `test`, `check`, `clean`, and `install-statusline` to
wire the producer into `~/.claude/settings.json`.

`make check` is deliberately light at stage 1, because the app holds no
credentials and no personal data — it guards against a token, key or captured
`~/.claude` settings dump being committed by accident. It gets teeth if and
when the endpoint route is adopted.
