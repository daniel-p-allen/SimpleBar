//! The consumer's pure functions.
//
// Split out of main.js so they can be unit tested without a DOM or a Tauri
// window — DESIGN.md asks for the state logic to be tested and the pixels not
// to be. Nothing in here touches the document or the Tauri bridge.

// Amber under 20% remaining, red under 10%, per DESIGN.md. Both bounds are
// inclusive: exactly 10 is red, exactly 20 is amber. Colour alone isn't the
// only signal — WCAG 1.4.1 — so the caller also drives a non-colour cue.
export function bandFor(remaining) {
  if (remaining <= 10) return "critical";
  if (remaining <= 20) return "low";
  return "normal";
}

/// Epoch seconds → "1:00 am", "10:00 pm".
///
/// The locale is pinned to en-US rather than the system's, because DESIGN.md
/// specifies a 12-hour clock and a system locale set to en-GB would render
/// "13:00". Only the *time zone* follows the machine; the format does not.
///
/// `timeZone` is for tests, which need a fixed zone to assert against. Left
/// undefined in the app so the reset time shows in the user's own zone.
export function formatResetTime(resetsAt, timeZone) {
  const options = { hour: "numeric", minute: "2-digit", hour12: true };
  if (timeZone) options.timeZone = timeZone;

  return (
    new Date(resetsAt * 1000)
      .toLocaleTimeString("en-US", options)
      // Recent ICU versions separate the meridiem with U+202F (narrow no-break
      // space), which is invisible in the window but would make a test
      // comparison against a plain space fail for no visible reason.
      .replace(/\s/g, " ")
      .toLowerCase()
  );
}
