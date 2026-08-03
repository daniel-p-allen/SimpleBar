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

/// Model and effort as one label: "Opus 5 · Medium".
///
/// The separator is a middle dot rather than a dash, and deliberately lighter
/// than the em dash in the reading above it: this line is subordinate, and the
/// effort qualifies the model rather than standing beside it. The spaces
/// around it are thin (U+2009), which binds the three tokens into one label
/// instead of leaving them floating apart.
///
/// Effort is stored lower case, as the blob gives it, so the casing happens
/// here. Falls back to the bare model name when there is no effort, and to an
/// empty string when there is no model — the caller renders that as a hidden
/// line rather than a blank one.
export function modelLabel(modelName, effortLevel) {
  if (!modelName) return "";
  if (!effortLevel) return modelName;

  // The thin spaces are escaped rather than literal: U+2009 is
  // indistinguishable from an ordinary space in an editor, so writing it
  // plainly invites a later edit to replace it without anyone noticing.
  return `${modelName}\u2009·\u2009${titleCase(effortLevel)}`;
}

function titleCase(value) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

/// How the mute button should present itself.
///
/// The glyph alone carries the state visually — speaker versus
/// speaker-with-a-slash — so no words are added to a face whose point is one
/// number. The label is for screen readers and names the *action*, not the
/// state: a button announced "Mute alerts" tells you what pressing it does,
/// while "Muted" leaves you guessing. `pressed` carries the state instead,
/// via aria-pressed.
export function muteButton(muted) {
  return {
    glyph: muted ? "🔇" : "🔊",
    label: muted ? "Unmute alerts" : "Mute alerts",
    pressed: muted ? "true" : "false",
  };
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
