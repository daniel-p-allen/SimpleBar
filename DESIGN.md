# SimpleBar — Design

A single-window macOS app that shows how much of your Claude 5-hour session
limit is left, as a circular gauge that drains as you use it.

Status: all milestones built — producer, window, file watch, wheel, beeps,
mute, the stale and no-data states, remembered window geometry, and M8
packaging (`make build` / `make run` / `make install-statusline`, an unsigned
local build). M5 was dropped by decision.

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
- On screen, coverable by other windows, alt-tabbable. Not always-on-top by
  default, but pinnable via a toggle — see "Always-on-top toggle" under
  decisions changed.
- Fixed translucency: the window background is ~95% see-through, set in the
  stylesheet. Not adjustable — see "Adjustable translucency" under decisions
  changed.
- Wheel starts full, drains clockwise as the session is consumed.
- Amber at 20% remaining or below, red at 10% or below (inclusive — exactly
  10% is red, not amber).
- One beep per threshold crossing, at **50, 80, 90 and 95 percent used**.

  Deliberately not the same points as the colours. Colour is a continuous,
  ambient signal and wants few states; a beep is a discrete interruption and
  can afford more. Tying them together would mean either inventing colours
  nobody asked for or dropping beeps that are wanted. The 50% beep is the
  "model-switch nudge" idea arriving early: early enough in the window that
  switching to a cheaper model still changes the outcome.
- Centre shows the percentage remaining, then the reset time on a 12-hour
  clock, lower case, no leading zero on the hour: "97% — resets 1:00 am",
  "9% — resets 10:00 pm". Below it, in a smaller font, the model active when
  the reading was taken, with its effort level: "Opus 5 · Medium". The effort
  is title-cased from the blob's lower case — simply, first letter only, so
  "xhigh" shows as "Xhigh". Accepted as-is, 2026-08-03: inventing prettier
  names would mean a lookup table that goes stale the moment Claude Code adds
  a level. Separated from the model by a middle dot with thin spaces —
  lighter than the em dash above, because the line is
  subordinate to the reading and effort qualifies the model rather than
  standing beside it. Falls back to the bare model name when effort is
  absent, and the line is omitted entirely when the model is absent too,
  rather than showing a blank line.

  The percentage is not decoration: it is the non-colour cue required by the
  accessibility rule below, so the amber and red states stay legible to
  someone who cannot separate them by hue.

  It is shown as a whole number, rounded — binary floating point cannot hold
  most decimals exactly, and `100 - 55.00000000000001` renders as
  `44.99999999999999` if left alone. The *rounded* figure is also what the
  colour band is judged on, so the number and the colour can never disagree
  about which band the reading is in.

  Below the model line, a speaker glyph mutes and unmutes the beeps (M6b).
  It is the only control on the face of a HUD whose point is one number, so
  it stays quiet: dimmed like the model line, and showing its state through
  the glyph itself — speaker versus speaker-with-a-slash — rather than
  through added words. It is a button, not decoration, so it carries an
  accessible name and is reachable by keyboard.

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

The beep is synthesised in the webview with the Web Audio API — a short
oscillator tone, no bundled sound file. Verified by spike, 2026-08-03: WebKit
plays it without the page ever having received a user gesture, so the alert
does not have to move to the Rust side or wait on a click that a HUD may never
get.

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

### Stale and no-data (M7)

A reading is **stale** once either is true:

- `resets_at` has passed — the window it described is over, so the figure
  describes nothing current; or
- nothing has been written for an hour.

Stale drops the colour *and* the number: grey wheel, no percentage, reading
`Open Claude Code to update`, with the age beneath it. The number is hidden
rather than dimmed because a figure on screen gets believed, and an hour-old
one will be wrong by an unknown amount.

The cost is accepted deliberately: the moment you most want this HUD is often
before starting work, which is exactly when the reading is oldest. An hour is
the compromise — a coffee break keeps the number, an abandoned afternoon does
not.

The wording is a *mechanism*, not a diagnosis. The app cannot tell whether
Claude Code is running; it only knows the file has not changed. "Claude Code
isn't running" would be a claim we cannot support and would read as plainly
wrong to someone with an idle session open. "Claude Code", not "Claude" —
opening claude.ai refreshes nothing, and sending a user there is a dead end.

The age carries as much date as it needs and no more:

| When the reading was taken | Shown |
|---|---|
| Today | `as of 2:14 pm` |
| Yesterday | `as of yesterday, 2:14 pm` |
| Earlier | `as of 1 Aug, 2:14 pm` |

Bare "as of 2:14 pm" on a two-day-old reading invites the reader to assume
today. Staleness is re-evaluated on a 60-second timer, so a window left open
crosses into stale on its own rather than waiting for a write that may never
come.

**No data at all** — no file yet, or unreadable — is a different state and says
so: grey wheel, `no data yet`, with `run Claude Code to start` beneath. There
is no number to protect, so the instruction can be direct.
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
  same reason as staleness. Three rules follow from that, and they are the
  whole of the alert logic:

  1. **A burst gives one beep, not a volley.** If a single update crosses
     several thresholds at once — 48% used to 92% after a long gap — only the
     highest crossed threshold sounds. Four beeps in a row conveys nothing the
     one beep does not.
  2. **Startup never beeps.** The reading at launch is taken as already
     announced, whatever it is. Only a transition *seen while running* fires.
     Otherwise reopening the window at 91% used would beep every time, which
     trains the user to ignore it.
  3. **Each threshold beeps at most once per window.** Sitting past a
     threshold is silent — it is the *crossing* that is news, not the state.
     Twenty updates at 91% used produce one beep between them, not twenty.
     Thresholds re-arm only when the window resets, detected by `resets_at`
     changing; usage falls only at a reset, so nothing else can legitimately
     re-arm one.
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
  change in the ticks).
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
- threshold crossing fires **once** per window, at 50/80/90/95 used, and
  re-arms only when `resets_at` changes. Specifically: sitting past a
  threshold is silent, a single update crossing several sounds only the
  highest, and the first reading after startup never fires
- `resets_at` epoch → 12-hour local clock string ("1:00 am", "10:00 pm"),
  including midnight, noon, and across a day boundary
- staleness from `written_at`
- no-data state

**Not unit tested**: the SVG itself and window translucency — checked by eye at
the milestone checkpoints. The beep is the same: that a tone is audible is
checked by ear, while *when* it fires is pure logic and is unit tested.

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
| M6a | Beep on crossing — 50/80/90/95 used | Fires once per crossing, not per update; a burst gives one beep; startup is silent |
| M6b | Mute toggle — speaker glyph under the model line | Muting survives a restart, via `config.json` |
| M7 | No-data and stale states | Grey wheel reading "no data yet" before the first write |
| M8 ✓ | Packaging — unsigned `.app` bundle, README, statusLine wiring instructions | **Done.** `make build` / `make run` / `make install-statusline` on your own Mac; installer merges into `settings.json` non-destructively. No signing (see "Decisions changed") |
| M9 | Window size and position remembered | Resize, quit, reopen — the window returns where it was |

M9 needs no new plumbing: `config.json` and its read/write path arrived with
M6b, so geometry is extra fields in a file that already exists. It can land
before M8 if convenient — a window that forgets its size is a poor first
impression on a clean install.

One edge case decided in advance: a window restored to coordinates on a
monitor that is no longer attached must be clamped back onto a visible screen.
Restoring it faithfully would open it invisibly, and the user's only clue
would be an app that appears not to launch.

Stage 2 (a sync or server so the number refreshes without Claude Code running)
and stage 3 (mobile) are deliberately out of scope and not designed here.

## Decisions changed

Recorded rather than edited away, so the reasoning behind a reversal is
visible to whoever reads this next.

- **The truck-tyre metaphor — abandoned, 2026-08-03.** The wheel was to be a
  truck tyre, with the outer dashed ring standing in for a knobby tread until
  real artwork replaced it. Abandoned: the placeholder reads as dial
  graduations, and a gauge is what the thing actually is — a single value
  draining around a circle. Chasing the tyre would have meant commissioning
  artwork to make the app look like something it was never behaving as.

  The dashed ring stays, renamed from `tread` to `ticks`, and is now the
  intended look rather than a stand-in for absent art.

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

- **Packaging stays unsigned — decided 2026-08-04.** M8 will produce an
  unsigned local `.app` via `tauri build`, nothing more. No paid Apple
  Developer account is in play, so code signing, notarization, and a build
  that opens cleanly on someone else's Mac are all out of scope — anything
  requiring the certificate is removed rather than stubbed. The repo ships as
  **source**: people clone and `make run`. An unsigned binary can be attached
  to a GitHub Release later, but it triggers Gatekeeper's "unidentified
  developer" warning on download, so it would owe a right-click-to-open note
  in the README. This is why the M8 acceptance is "runs on your own Mac", not
  the old "installs on a clean account".

- **Always-on-top toggle — added 2026-08-04.** Stage 1 originally ruled out
  always-on-top ("later stages"), on the view that a HUD should sit among your
  windows and be alt-tabbed to, not hover over everything. Reversed in
  fine-tuning: a drain gauge you glance at is exactly the kind of thing some
  people want pinned above their editor, and the cost is small because the
  mute toggle (M6b) already established the whole pattern — a glyph button, a
  Tauri command, and a field persisted to `config.json` and restored on
  launch. It stays **off by default**, so the original default is unchanged;
  pinning is opt-in and remembered. Implemented by cloning the mute toggle,
  calling `WebviewWindow::set_always_on_top`.

  The control is a single SVG pin (not an emoji, so CSS can colour it), sat
  beside the mute button. State is shown two ways, so it survives colour
  blindness: **colour** — green (`--band-normal`) when pinned, red
  (`--band-critical`) when not — reusing the wheel's existing palette; and
  **shape** — a diagonal slash through the pin when unpinned, cleared when
  pinned, the universal "off" cue. `aria-pressed` carries the state to screen
  readers.

  Pinning is a single `set_always_on_top`, and its reach is deliberately
  limited. It floats above ordinary windows, including one dragged out to fill
  the whole screen. It does **not** appear over another app's *native*-fullscreen
  Space (the green-button/⌃⌘F kind that swipes as its own desktop). That was
  attempted and rolled back: macOS only lets an accessory (Dock-less) app or a
  nonactivating `NSPanel` intrude on another app's fullscreen Space, and the
  route there — an `objc2-app-kit` FFI setting `FullScreenAuxiliary` and a raised
  window level — still failed without also giving up the Dock icon. Not worth
  it: stretch the window instead of true-fullscreening the terminal, and the
  pin works.

- **The unmute toggle confirms itself with a tone — added 2026-08-05.** M6b
  left the mute button silent in both directions, which makes unmuting an act
  of faith: nothing happens until the next threshold is crossed, which may be
  hours away, so a broken audio path is indistinguishable from a quiet
  session. Unmuting now plays a short tone as its own confirmation.

  **Only unmuting sounds.** A tone confirming that the app has just been
  silenced contradicts the request, and mute-on is precisely the moment the
  user has asked for quiet. Unmute-only still carries the whole signal, since
  the tone is the evidence that audio works and is on.

  The tone is deliberately *not* the alert beep. Same oscillator, but quieter
  and half the length (0.08 gain, 0.12s, against the alert's 0.15 and 0.25s),
  so a threshold crossing stays distinguishable from a button press. An alert
  the user has been trained to hear as "I clicked something" is worse than no
  alert at all.

## Open, to follow up

- **Lighter ticks in light mode — done by preference, 2026-08-04.** Dan
  disliked the near-black (`#2b2b2b`) ticks, so light-mode `--ticks` is now
  `#d9d9d9`, matching the backdrop-disc grey. The dark-mode ticks are
  unchanged. The contrast tradeoff below was raised and consciously accepted:
  against a light desktop these ticks can drop under 3:1. Doing it *properly*
  (rather than by taste) would still mean giving the ticks a background they
  can rely on rather than the desktop — extending the backdrop disc under the
  ring, or outlining the ticks — but that is deferred, not planned.

- **The contrast claim is not currently true.** "Standards adopted" cites WCAG
  1.4.11 (3:1 for non-text), and measured against the two extremes a
  95%-transparent window can sit on, most strokes fail: light-mode ticks are
  now `#d9d9d9` (~1.2:1 on a white desktop), dark-mode ticks 1.48:1 on black,
  amber 2.72:1 on white. This is structural rather than a bad colour pick — no
  single colour clears 3:1 against both a white and a black desktop, and the
  2026-08-04 preference change traded the light-mode ticks further away from
  the target on purpose. Either give the strokes a known background, or narrow
  the claim to say contrast is guaranteed only within the backdrop disc, where
  the text lives.

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
