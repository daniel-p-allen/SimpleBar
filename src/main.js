import { bandFor, formatResetTime } from "./format.js";

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
  modelEl.textContent = usage.model_name ?? "";
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
  await listen("usage-changed", (event) => render(event.payload));

  try {
    render(await invoke("read_usage"));
  } catch (err) {
    renderNoData();
  }
});
