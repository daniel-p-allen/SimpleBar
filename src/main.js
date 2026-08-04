import {
  bandFor,
  formatAsOf,
  formatResetTime,
  isStale,
  modelLabel,
  muteButton,
  pinButton,
  remainingPercent,
} from "./format.js";
import { initialAlertState, nextAlertState } from "./alerts.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// The progress circle's radius (matches r="72" in index.html) — used to turn
// a percentage into a stroke-dashoffset.
const RADIUS = 72;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/// Seconds since the epoch. Kept in one place so the staleness check and the
/// age string cannot disagree about what "now" is.
function nowSeconds() {
  return Math.floor(Date.now() / 1000);
}

/// The reading is too old to show as a number: grey wheel, no percentage, and
/// what to do about it.
///
/// The number is hidden rather than dimmed because a figure on screen gets
/// believed, and an hour-old one is wrong by an unknown amount. The wording
/// describes the mechanism rather than diagnosing a cause — the app cannot
/// tell whether Claude Code is running, only that the file has not changed.
function renderStale(usage) {
  // Its own band, not "no-data": the two states look different because they
  // mean different things. Stale hides a reading we *had*, so it stays grey —
  // a full ring there would overwrite a known low number with a full-looking
  // one. No-data never had a reading, and can afford to look inviting.
  document.querySelector("#wheel-container").dataset.band = "stale";
  document.querySelector("#reading").textContent = "Open Claude Code to update";
  document.querySelector("#model").textContent = formatAsOf(usage.written_at, nowSeconds());
}

function render(usage) {
  const progressEl = document.querySelector("#progress");
  const readingEl = document.querySelector("#reading");
  const modelEl = document.querySelector("#model");
  const wheelEl = document.querySelector("#wheel-container");

  const remaining = remainingPercent(usage.used_percentage);
  const resets = formatResetTime(usage.resets_at);

  const offset = CIRCUMFERENCE * (1 - remaining / 100);
  progressEl.style.strokeDasharray = `${CIRCUMFERENCE}`;
  progressEl.style.strokeDashoffset = `${offset}`;

  const band = bandFor(remaining);
  wheelEl.dataset.band = band;

  readingEl.textContent = `${remaining}% — resets ${resets}`;
  // Purely cosmetic — omit the line entirely rather than show it blank.
  modelEl.textContent = modelLabel(usage.model_name, usage.effort_level);
}

// One context for the app's lifetime — creating one per beep leaks audio
// hardware handles, and WebKit caps how many a page may hold.
let audio = null;

/// A short tone. Deliberately synthesised rather than bundled: no asset to
/// ship, and nothing to load before the first beep can sound.
///
/// `peak` and `seconds` exist so the unmute confirmation can borrow the same
/// oscillator while staying audibly distinct from an alert — see `tone` calls
/// below for which is which.
function beep(peak = 0.15, seconds = 0.25) {
  audio ??= new AudioContext();

  const osc = audio.createOscillator();
  const gain = audio.createGain();
  osc.frequency.value = 880;
  osc.connect(gain).connect(audio.destination);

  // Ramped rather than switched: an oscillator stopped at full amplitude
  // ends on a click, which sounds like a fault rather than a notification.
  const now = audio.currentTime;
  gain.gain.setValueAtTime(0, now);
  gain.gain.linearRampToValueAtTime(peak, now + 0.01);
  gain.gain.linearRampToValueAtTime(0, now + seconds);

  osc.start(now);
  osc.stop(now + seconds);
}

/// The tone that confirms unmuting. Quieter and shorter than the alert on
/// purpose: a threshold crossing must never sound like a button press, or the
/// user learns to hear the alert as something they caused.
function confirmTone() {
  beep(0.08, 0.12);
}

// Null until the first reading. initialAlertState then takes that reading as
// already announced, which is what keeps startup silent.
let alertState = null;

// Mirrors config.json. Read once at startup; the Rust side owns the file.
let muted = false;

function renderMute() {
  const { glyph, label, pressed } = muteButton(muted);
  document.querySelector("#mute-glyph").textContent = glyph;

  const button = document.querySelector("#mute");
  button.setAttribute("aria-label", label);
  button.setAttribute("aria-pressed", pressed);
}

/// Flips the mute setting, persisting before the UI moves.
///
/// Saved first, then drawn — the opposite order would let the glyph show a
/// setting that never reached disk, and the user would find it forgotten on
/// the next launch with nothing to explain why. The write is a local file, so
/// the wait is imperceptible.
async function toggleMute() {
  try {
    const config = await invoke("set_muted", { muted: !muted });
    muted = config.muted;
    renderMute();

    // Only unmuting sounds. A tone acknowledging that the app has just been
    // silenced argues with the request; unmuting, on the other hand, has no
    // other confirmation until the next crossing, which may be hours away.
    if (!muted) confirmTone();
  } catch (err) {
    // Leave the glyph showing what is actually stored. The alternative — a
    // glyph that lies — is worse than a click that appears not to work.
    console.error("could not save mute setting:", err);
  }
}

// Mirrors config.json, like `muted`. Read once at startup; the Rust side owns
// the file and applying the setting to the live window.
let pinned = false;

function renderPin() {
  const { label, pressed } = pinButton(pinned);
  const button = document.querySelector("#pin");
  button.setAttribute("aria-label", label);
  // The stylesheet keys the colour and the slash off aria-pressed, so setting
  // it here is all the visual state the button needs.
  button.setAttribute("aria-pressed", pressed);
}

/// Flips always-on-top. Saved before drawn, for the reason toggleMute is.
///
/// The Rust command both moves the live window and persists the choice, so on
/// success the returned config is the source of truth for the glyph.
async function togglePin() {
  try {
    const config = await invoke("set_pinned", { pinned: !pinned });
    pinned = config.pinned;
    renderPin();
  } catch (err) {
    console.error("could not save pin setting:", err);
  }
}

// The last reading received, kept so the staleness timer can re-draw without
// waiting for a write that may never come.
let lastUsage = null;

/// Draws whichever of the three states applies to the reading we hold.
function draw() {
  if (lastUsage === null) {
    renderNoData();
  } else if (isStale(lastUsage, nowSeconds())) {
    renderStale(lastUsage);
  } else {
    render(lastUsage);
  }
}

/// Takes a new reading: sounds a beep if it crossed a threshold, then draws.
///
/// Alerting is kept out of the drawing: draw() also runs on the opening frame
/// and on a timer, and mixing the two is how a HUD ends up beeping every time
/// it opens or every minute it sits idle.
function update(usage) {
  lastUsage = usage;

  if (alertState === null) {
    alertState = initialAlertState(usage);
  } else {
    const { state, beepAt } = nextAlertState(alertState, usage);
    alertState = state;
    // Crossings are still recorded while muted — unmuting must not replay
    // every threshold the session already passed.
    if (beepAt !== null && !muted) beep();
  }

  draw();
}

/// Nothing has ever been written, or the file is unreadable. A different state
/// from stale, and it says so: there is no number to protect, so the
/// instruction can be direct.
function renderNoData() {
  document.querySelector("#wheel-container").dataset.band = "no-data";
  document.querySelector("#reading").textContent = "no data yet";
  document.querySelector("#model").textContent = "run Claude Code to start";
}

// Read once for the opening frame, then let the watcher drive. The startup
// read is still needed: the file only changes while Claude Code is running, so
// without it an idle session would show nothing until the next prompt.
window.addEventListener("DOMContentLoaded", async () => {
  // Subscribe before the first read, so a write landing between the two is
  // delivered rather than dropped.
  await listen("usage-changed", (event) => update(event.payload));

  // Before the first reading, so a threshold crossed by that reading cannot
  // beep while the app still believes it is unmuted. One read serves both
  // toggles. The Rust side has already applied the pin to the live window at
  // startup; this only brings the glyph into line with it.
  const config = await invoke("read_config");
  muted = config.muted;
  pinned = config.pinned;
  renderMute();
  renderPin();
  document.querySelector("#mute").addEventListener("click", toggleMute);
  document.querySelector("#pin").addEventListener("click", togglePin);

  try {
    update(await invoke("read_usage"));
  } catch (err) {
    renderNoData();
  }

  // A reading goes stale by the clock, not by anything arriving — so without
  // this a window left open would keep showing a number that quietly stopped
  // being true. Sixty seconds is the resolution DESIGN.md's performance
  // budget allows, and it is the only timer in the app.
  setInterval(draw, 60_000);
});
