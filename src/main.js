const { invoke } = window.__TAURI__.core;

// The progress circle's radius (matches r="72" in index.html) — used to turn
// a percentage into a stroke-dashoffset.
const RADIUS = 72;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

// Amber under 20% remaining, red under 10%, per DESIGN.md. Colour alone
// isn't the only signal — WCAG 1.4.1 — so the wheel class also drives the
// "low"/"critical" state on #center-text for a non-colour cue.
function bandFor(remaining) {
  if (remaining <= 10) return "critical";
  if (remaining <= 20) return "low";
  return "normal";
}

function render(usage) {
  const progressEl = document.querySelector("#progress");
  const readingEl = document.querySelector("#reading");
  const modelEl = document.querySelector("#model");
  const wheelEl = document.querySelector("#wheel-container");

  const remaining = 100 - usage.used_percentage;
  const resets = new Date(usage.resets_at * 1000).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });

  const offset = CIRCUMFERENCE * (1 - remaining / 100);
  progressEl.style.strokeDasharray = `${CIRCUMFERENCE}`;
  progressEl.style.strokeDashoffset = `${offset}`;

  const band = bandFor(remaining);
  wheelEl.dataset.band = band;

  readingEl.textContent = `${remaining}% - Resets @ ${resets}`;
  // Purely cosmetic — omit the line entirely rather than show it blank.
  modelEl.textContent = usage.model_name ?? "";
}

function renderNoData() {
  document.querySelector("#wheel-container").dataset.band = "no-data";
  document.querySelector("#reading").textContent = "no data yet";
  document.querySelector("#model").textContent = "";
}

// M2: read once at startup. No file watching yet (M3) — the wheel is a
// snapshot of whatever the reading was when this window opened.
window.addEventListener("DOMContentLoaded", async () => {
  try {
    render(await invoke("read_usage"));
  } catch (err) {
    renderNoData();
  }
});
