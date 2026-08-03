import { bandFor, formatResetTime, modelLabel } from "./format.js";
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
    if (beepAt !== null) beep();
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

  try {
    update(await invoke("read_usage"));
  } catch (err) {
    renderNoData();
  }
});
