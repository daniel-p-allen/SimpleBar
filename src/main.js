const { invoke } = window.__TAURI__.core;

// M2: read once at startup, show it as plain text. No file watching (M3),
// no wheel (M4) — this exists only to prove the number reaches the window.
window.addEventListener("DOMContentLoaded", async () => {
  const el = document.querySelector("#reading");
  try {
    const usage = await invoke("read_usage");
    const remaining = 100 - usage.used_percentage;
    const resets = new Date(usage.resets_at * 1000).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
    el.textContent = `${remaining}% left · resets ${resets}`;
  } catch (err) {
    el.textContent = "no data yet";
  }
});
