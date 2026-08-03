#!/usr/bin/env node
/**
 * Tests for the consumer's pure functions.
 *
 * DESIGN.md asks for the state logic to be tested and the pixels not to be, so
 * this covers the colour bands and the clock string only — the SVG and the
 * translucency are checked by eye at the milestone checkpoints.
 *
 * The band boundaries are inclusive and easy to get wrong by one, and the
 * clock has to survive midnight and noon, which is where 12-hour formatting
 * usually breaks. Both are asserted here rather than trusted.
 *
 * No test framework: plain asserts, so `make test` needs nothing installed.
 */

import {
  bandFor,
  formatResetTime,
  modelLabel,
  muteButton,
  remainingPercent,
} from "../src/format.js";

const failures = [];

function check(name, actual, expected) {
  if (actual === expected) {
    console.log(`  ok   ${name}`);
  } else {
    console.log(`  FAIL ${name}: expected ${expected}, got ${actual}`);
    failures.push(name);
  }
}

// A fixed zone, so the expected strings don't depend on where this runs.
const UTC = "UTC";

// Epoch seconds for a known UTC wall-clock time on 2026-08-03.
function at(hour, minute = 0) {
  return Math.floor(Date.UTC(2026, 7, 3, hour, minute) / 1000);
}

console.log("test_colour_bands");
check("100% is normal", bandFor(100), "normal");
check("50% is normal", bandFor(50), "normal");
check("21% is normal — just above the amber bound", bandFor(21), "normal");
// DESIGN.md: "Amber at 20% remaining or below" — inclusive.
check("exactly 20% is amber, not normal", bandFor(20), "low");
check("19% is amber", bandFor(19), "low");
check("11% is amber — just above the red bound", bandFor(11), "low");
// DESIGN.md: "red at 10% or below (inclusive — exactly 10% is red, not amber)".
check("exactly 10% is red, not amber", bandFor(10), "critical");
check("9% is red", bandFor(9), "critical");
check("0% is red", bandFor(0), "critical");

console.log("test_reset_time_formatting");
check("1am has no leading zero", formatResetTime(at(1), UTC), "1:00 am");
check("10pm is lower case", formatResetTime(at(22), UTC), "10:00 pm");
check("minutes keep their leading zero", formatResetTime(at(13, 5), UTC), "1:05 pm");
// Midnight and noon are where 12-hour clocks usually render "0:00" or flip the
// meridiem — DESIGN.md calls both out explicitly.
check("midnight is 12:00 am, not 0:00", formatResetTime(at(0), UTC), "12:00 am");
check("noon is 12:00 pm, not 12:00 am", formatResetTime(at(12), UTC), "12:00 pm");
check("half past midnight", formatResetTime(at(0, 30), UTC), "12:30 am");

console.log("test_reset_time_across_a_day_boundary");
// A reset just after midnight belongs to the next day. The string carries no
// date, so what matters is that the time is right and does not wrap to 24:00.
check("00:40 the next day", formatResetTime(at(24, 40), UTC), "12:40 am");
check("11:59 pm", formatResetTime(at(23, 59), UTC), "11:59 pm");

console.log("test_format_ignores_system_locale");
// DESIGN.md specifies a 12-hour clock. A machine set to en-GB must still get
// one, so the locale is pinned rather than taken from the system.
check("13:00 renders as 1:00 pm", formatResetTime(at(13), UTC), "1:00 pm");

console.log("test_model_label");
// U+2009 thin spaces, not ordinary ones — asserted explicitly, because the
// difference is invisible and a later edit could quietly undo it.
check(
  "model and effort, thin-spaced middle dot",
  modelLabel("Opus 5", "medium"),
  "Opus 5 · Medium",
);
check("effort is title-cased from the blob's lower case", modelLabel("Opus 5", "high"), "Opus 5 · High");
// DESIGN.md: falls back to the bare model name when effort is absent, and the
// line is omitted entirely when the model is absent too.
check("no effort falls back to the model alone", modelLabel("Opus 5", null), "Opus 5");
check("effort absent as undefined, not null", modelLabel("Opus 5", undefined), "Opus 5");
check("no model gives an empty line, not a stray dot", modelLabel(null, "medium"), "");
check("neither gives an empty line", modelLabel(null, null), "");
// An older usage.json has neither field, and a reading is still worth showing.
check("empty strings are treated as absent", modelLabel("", ""), "");

console.log("test_remaining_percent");
// The bug this exists for: the window rendered "44.99999999999999%".
check("floating point noise is rounded away", remainingPercent(55.00000000000001), 45);
check("a whole number is unchanged", remainingPercent(17), 83);
check("a half rounds up", remainingPercent(50.5), 50);
check("nothing used is 100", remainingPercent(0), 100);
check("all used is 0", remainingPercent(100), 0);
// The rounded value drives the colour band too, so the two cannot contradict.
check("rounds onto the red boundary", bandFor(remainingPercent(90.4)), "critical");
check("rounds onto the amber boundary", bandFor(remainingPercent(80.3)), "low");

console.log("test_mute_button");
check("unmuted shows a speaker", muteButton(false).glyph, "🔊");
check("muted shows a crossed speaker", muteButton(true).glyph, "🔇");
// The label names the action, not the state — "Muted" would leave a screen
// reader user guessing what pressing it does. State rides on aria-pressed.
check("unmuted label names the action", muteButton(false).label, "Mute alerts");
check("muted label names the action", muteButton(true).label, "Unmute alerts");
check("aria-pressed is false when unmuted", muteButton(false).pressed, "false");
check("aria-pressed is true when muted", muteButton(true).pressed, "true");
// Glyphs must differ, or the only visual signal of state disappears.
check("the two glyphs differ", muteButton(false).glyph !== muteButton(true).glyph, true);

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nall passed");
