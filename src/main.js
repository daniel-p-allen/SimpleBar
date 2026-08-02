const { invoke } = window.__TAURI__.core;

// M2: read once at startup, show it as plain text. No file watching (M3),
// no wheel (M4) — this exists only to prove the number reaches the window.
window.addEventListener("DOMContentLoaded", async () => {
  const readingEl = document.querySelector("#reading");
  const modelEl = document.querySelector("#model");
  try {
    const usage = await invoke("read_usage");
    const remaining = 100 - usage.used_percentage;
    const resets = new Date(usage.resets_at * 1000).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
    readingEl.textContent = `${remaining}% - Resets @ ${resets}`;
    // Purely cosmetic — omit the line entirely rather than show it blank.
    modelEl.textContent = usage.model_name ?? "";
  } catch (err) {
    readingEl.textContent = "no data yet";
  }
});
