#!/usr/bin/env node
/**
 * Tests for when the beep fires.
 *
 * DESIGN.md's three alert rules are all consequences of readings arriving late
 * and bunched, and each one is a way the naive implementation gets it wrong:
 * beeping on every update, beeping four times for one jump, or beeping the
 * moment the window opens. All three are asserted here.
 *
 * No test framework: plain asserts, so `make test` needs nothing installed.
 */

import { initialAlertState, nextAlertState, THRESHOLDS } from "../src/alerts.js";

const failures = [];

function check(name, actual, expected) {
  const [a, e] = [JSON.stringify(actual), JSON.stringify(expected)];
  if (a === e) {
    console.log(`  ok   ${name}`);
  } else {
    console.log(`  FAIL ${name}: expected ${e}, got ${a}`);
    failures.push(name);
  }
}

const WINDOW = 1785768000;
const LATER = 1785786000;

function reading(used, resetsAt = WINDOW) {
  return { used_percentage: used, resets_at: resetsAt };
}

/// Feeds readings in order from a cold start, returning every beep fired.
function beepsFor(...readings) {
  let state = initialAlertState(readings[0]);
  const beeps = [];
  for (const r of readings.slice(1)) {
    const next = nextAlertState(state, r);
    state = next.state;
    if (next.beepAt !== null) beeps.push(next.beepAt);
  }
  return beeps;
}

console.log("test_thresholds_are_used_not_remaining");
check("the four thresholds", THRESHOLDS, [50, 80, 90, 95]);

console.log("test_crossing_fires_once");
check("crossing 50 fires once", beepsFor(reading(10), reading(51)), [50]);
check("each threshold in turn", beepsFor(reading(0), reading(50), reading(80), reading(90), reading(95)), [50, 80, 90, 95]);
// The rule that matters most: it is the crossing that is news, not the state.
check("sitting past a threshold is silent", beepsFor(reading(10), reading(51), reading(52), reading(60), reading(79)), [50]);
check("twenty updates at 91 give one beep", beepsFor(reading(10), ...Array(20).fill(reading(91))), [90]);
check("exactly on the threshold counts as crossed", beepsFor(reading(10), reading(50)), [50]);

console.log("test_a_burst_sounds_only_the_highest");
// 48 -> 92 crosses 50, 80 and 90 in one update.
check("one update crossing three fires once", beepsFor(reading(48), reading(92)), [90]);
check("the ones passed silently stay silent", beepsFor(reading(48), reading(92), reading(93), reading(94)), [90]);
check("but a later threshold still fires", beepsFor(reading(48), reading(92), reading(96)), [90, 95]);

console.log("test_startup_never_beeps");
// Reopening the window at 91% used must not beep, or the beep becomes noise.
check("opening at 91 is silent", beepsFor(reading(91)), []);
check("opening at 100 is silent", beepsFor(reading(100)), []);
check("and the passed thresholds stay silent afterwards", beepsFor(reading(91), reading(92), reading(93)), []);
check("but a threshold not yet passed still fires", beepsFor(reading(91), reading(96)), [95]);

console.log("test_reset_rearms");
// Usage falls only at a reset, so resets_at changing is the one legitimate
// re-arm. The first reading of a new window is real news, not a startup.
check("a new window re-arms", beepsFor(reading(10), reading(51), reading(5, LATER), reading(52, LATER)), [50, 50]);
check("new window already past 50 fires", beepsFor(reading(51), reading(60, LATER)), [50]);
check("same window does not re-arm", beepsFor(reading(10), reading(51), reading(51)), [50]);

console.log("test_missing_readings_are_held_not_treated_as_a_reset");
// A missing reading must not re-arm anything — treating it as a reset would
// make the next update beep all over again.
const armed = nextAlertState(initialAlertState(reading(10)), reading(51)).state;
check("undefined reading is silent", nextAlertState(armed, undefined).beepAt, null);
check("reading with no percentage is silent", nextAlertState(armed, { resets_at: WINDOW }).beepAt, null);
check("state survives a missing reading", nextAlertState(nextAlertState(armed, undefined).state, reading(52)).beepAt, null);

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nall passed");
