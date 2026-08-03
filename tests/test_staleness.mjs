#!/usr/bin/env node
/**
 * Tests for when a reading is too old to believe, and how its age reads.
 *
 * The whole point of the stale state is that the HUD must never show an old
 * number as if it were live. Both halves are pure and both have edges worth
 * pinning: staleness has two independent triggers, and the age string has to
 * survive a reading taken last night being read this morning.
 *
 * No test framework: plain asserts, so `make test` needs nothing installed.
 */

import { formatAsOf, isStale } from "../src/format.js";

const failures = [];

function check(name, actual, expected) {
  if (actual === expected) {
    console.log(`  ok   ${name}`);
  } else {
    console.log(`  FAIL ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
    failures.push(name);
  }
}

const UTC = "UTC";
const HOUR = 3600;

/// 2026-08-03 14:00 UTC — "now" for these tests.
const NOW = Math.floor(Date.UTC(2026, 7, 3, 14, 0) / 1000);

function reading({ writtenAt = NOW, resetsAt = NOW + HOUR } = {}) {
  return { used_percentage: 40, written_at: writtenAt, resets_at: resetsAt };
}

console.log("test_fresh_readings_are_not_stale");
check("just written", isStale(reading(), NOW), false);
check("ten minutes old", isStale(reading({ writtenAt: NOW - 600 }), NOW), false);
// A coffee break must keep the number — that is the whole point of the hour.
check("fifty-nine minutes old", isStale(reading({ writtenAt: NOW - 59 * 60 }), NOW), false);

console.log("test_an_hour_without_a_write_is_stale");
check("exactly an hour", isStale(reading({ writtenAt: NOW - HOUR }), NOW), true);
check("an abandoned afternoon", isStale(reading({ writtenAt: NOW - 5 * HOUR }), NOW), true);

console.log("test_a_finished_window_is_stale");
// The window it described is over, so the figure describes nothing current —
// even though it was written seconds ago.
check("resets_at just passed", isStale(reading({ resetsAt: NOW - 1 }), NOW), true);
check("resets_at exactly now", isStale(reading({ resetsAt: NOW }), NOW), true);
check("resets_at still ahead", isStale(reading({ resetsAt: NOW + 1 }), NOW), false);

console.log("test_missing_readings_are_stale");
// No reading is not a fresh reading. Defaulting the other way would show a
// live-looking wheel with nothing behind it.
check("undefined", isStale(undefined, NOW), true);
check("null", isStale(null, NOW), true);
check("no written_at", isStale({ resets_at: NOW + HOUR }, NOW), true);

console.log("test_age_carries_only_the_date_it_needs");
check("earlier today", formatAsOf(NOW - 3 * HOUR, NOW, UTC), "as of 11:00 am");
check("yesterday", formatAsOf(NOW - 24 * HOUR, NOW, UTC), "as of yesterday, 2:00 pm");
check("last week", formatAsOf(NOW - 6 * 24 * HOUR, NOW, UTC), "as of 28 Jul, 2:00 pm");

console.log("test_age_uses_calendar_days_not_elapsed_hours");
// 11pm read at 1am is two hours old but genuinely yesterday. Calling it "as of
// 11:00 pm" invites the reader to think it was tonight.
const oneAm = Math.floor(Date.UTC(2026, 7, 3, 1, 0) / 1000);
const elevenPmYesterday = Math.floor(Date.UTC(2026, 7, 2, 23, 0) / 1000);
check("two hours ago, but yesterday", formatAsOf(elevenPmYesterday, oneAm, UTC), "as of yesterday, 11:00 pm");
// And the reverse: 23 hours apart can still be the same calendar day.
const oneAmSameDay = Math.floor(Date.UTC(2026, 7, 3, 0, 30) / 1000);
check("same day despite a long gap", formatAsOf(oneAmSameDay, NOW, UTC), "as of 12:30 am");

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nall passed");
