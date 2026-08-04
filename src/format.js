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

/// Percent remaining, as a whole number.
///
/// Rounded because binary floating point cannot hold most decimals exactly:
/// 100 - 55.00000000000001 is 44.99999999999999, and the window rendered
/// every digit of it. The producer already rounds for the status line, so
/// this also keeps the two displays saying the same thing.
///
/// The rounded value is what the colour band is judged on too, not the raw
/// one. If they disagreed, a reading of "10%" could sit in the amber band
/// while DESIGN.md says exactly 10 is red — and since the number is the
/// non-colour cue for that very rule, the two must never contradict.
export function remainingPercent(usedPercentage) {
  return Math.round(100 - usedPercentage);
}

/// An hour without a write makes a reading stale. See STALE below.
const STALE_AFTER_SECONDS = 60 * 60;

/// Whether a reading is too old to show as a number.
///
/// Two ways to go stale, per DESIGN.md. Either the window it described is
/// over — `resets_at` in the past, so the figure describes nothing current —
/// or nothing has been written for an hour.
///
/// The hour is a compromise, and the trade-off is real: the moment the HUD is
/// most wanted is often before starting work, which is exactly when the
/// reading is oldest. A coffee break keeps the number; an abandoned afternoon
/// does not.
///
/// `now` is passed in rather than read from the clock, so this stays pure.
export function isStale(reading, now) {
  if (!reading || typeof reading.written_at !== "number") return true;

  if (typeof reading.resets_at === "number" && now >= reading.resets_at) return true;

  return now - reading.written_at >= STALE_AFTER_SECONDS;
}

/// "as of 2:14 pm", with as much date as it takes to be unambiguous.
///
/// A bare clock time on a two-day-old reading invites the reader to assume
/// today, which is the confusion the whole stale state exists to prevent. So
/// yesterday says so, and anything older carries its date.
///
/// Compares calendar days, not elapsed hours: a reading from 11pm viewed at
/// 1am is two hours old but genuinely *yesterday*, and saying so is clearer
/// than "as of 11:00 pm" on what the reader thinks of as today.
export function formatAsOf(writtenAt, now, timeZone) {
  const when = new Date(writtenAt * 1000);
  const today = new Date(now * 1000);

  const time = formatResetTime(writtenAt, timeZone);
  const dayDelta = calendarDaysApart(when, today, timeZone);

  if (dayDelta === 0) return `as of ${time}`;
  if (dayDelta === 1) return `as of yesterday, ${time}`;

  const options = { day: "numeric", month: "short" };
  if (timeZone) options.timeZone = timeZone;
  return `as of ${when.toLocaleDateString("en-GB", options)}, ${time}`;
}

/// Whole calendar days between two dates, in the given zone.
function calendarDaysApart(earlier, later, timeZone) {
  const options = { year: "numeric", month: "2-digit", day: "2-digit" };
  if (timeZone) options.timeZone = timeZone;

  // en-CA gives YYYY-MM-DD, which subtracts correctly as a date.
  const toDay = (d) => new Date(`${d.toLocaleDateString("en-CA", options)}T00:00:00Z`);
  return Math.round((toDay(later) - toDay(earlier)) / 86400000);
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

/// How the pin button should present itself.
///
/// The glyph is a fixed SVG pin in the markup; state is carried two visible
/// ways the stylesheet keys off `aria-pressed` — colour (green pinned, red
/// not) and a diagonal slash shown only when unpinned. Colour alone would be
/// invisible to a red-green colourblind user, so the slash is the redundant
/// cue. The label names the *action*, like the mute button, and `pressed`
/// carries the state to screen readers.
export function pinButton(pinned) {
  return {
    label: pinned ? "Unpin from top" : "Pin on top",
    pressed: pinned ? "true" : "false",
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
