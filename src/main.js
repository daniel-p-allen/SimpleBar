import { bandFor, formatResetTime, modelLabel, muteButton } from "./format.js";
import { initialAlertState, nextAlertState } from "./alerts.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// The progress circle's radius (matches r="72" in index.html) — used to turn
// a percentage into a stroke-dashoffset.
const RADIUS = 72;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

function render(usage) {
  const progressEl = document.querySelector("#progress");
  const readingEl = document.querySelector("#reading");
  const modelEl = document.querySelector("#model");
  const wheelEl = document.querySelector("#wheel-container");

  const remaining = 100 - usage.used_percentage;
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
function beep() {
  audio ??= new AudioContext();

  const osc = audio.createOscillator();
  const gain = audio.createGain();
  osc.frequency.value = 880;
  osc.connect(gain).connect(audio.destination);

  // Ramped rather than switched: an oscillator stopped at full amplitude
  // ends on a click, which sounds like a fault rather than a notification.
  const now = audio.currentTime;
  gain.gain.setValueAtTime(0, now);
  gain.gain.linearRampToValueAtTime(0.15, now + 0.01);
  gain.gain.linearRampToValueAtTime(0, now + 0.25);

  osc.start(now);
  osc.stop(now + 0.25);
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
  } catch (err) {
    // Leave the glyph showing what is actually stored. The alternative — a
    // glyph that lies — is worse than a click that appears not to work.
    console.error("could not save mute setting:", err);
  }
}

/// Draws a reading, and sounds a beep if it crossed a threshold.
///
/// Alerting is kept out of render(): render is called for the opening frame
/// too, and mixing the two is how a HUD ends up beeping every time it opens.
function update(usage) {
  if (alertState === null) {
    alertState = initialAlertState(usage);
  } else {
    const { state, beepAt } = nextAlertState(alertState, usage);
    alertState = state;
    // Crossings are still recorded while muted — unmuting must not replay
    // every threshold the session already passed.
    if (beepAt !== null && !muted) beep();
  }

  render(usage);
}

function renderNoData() {
  document.querySelector("#wheel-container").dataset.band = "no-data";
  document.querySelector("#reading").textContent = "no data yet";
  document.querySelector("#model").textContent = "";
}

// Read once for the opening frame, then let the watcher drive. The startup
// read is still needed: the file only changes while Claude Code is running, so
// without it an idle session would show nothing until the next prompt.
window.addEventListener("DOMContentLoaded", async () => {
  // Subscribe before the first read, so a write landing between the two is
  // delivered rather than dropped.
  await listen("usage-changed", (event) => update(event.payload));

  // Before the first reading, so a threshold crossed by that reading cannot
  // beep while the app still believes it is unmuted.
  muted = (await invoke("read_config")).muted;
  renderMute();
  document.querySelector("#mute").addEventListener("click", toggleMute);

  try {
    update(await invoke("read_usage"));
  } catch (err) {
    renderNoData();
  }
});
