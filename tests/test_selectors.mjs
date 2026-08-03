#!/usr/bin/env node
/**
 * Every selector the app relies on must actually exist.
 *
 * The webview fails silently at both seams. A class in index.html with no rule
 * in styles.css draws nothing and reports nothing — rename `tread` to `ticks`
 * in one file and the gauge's graduations simply vanish. A querySelector in
 * main.js that matches nothing returns null, and the TypeError that follows is
 * buried in a devtools console nobody has open.
 *
 * So this walks the three files and checks they agree. Deliberately textual
 * rather than a real parser: no dependency to install, and the markup is
 * fifteen elements written by hand.
 *
 * No test framework: plain asserts, so `make test` needs nothing installed.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const SRC = join(dirname(fileURLToPath(import.meta.url)), "..", "src");
const html = readFileSync(join(SRC, "index.html"), "utf8");
const css = readFileSync(join(SRC, "styles.css"), "utf8");
const js = readFileSync(join(SRC, "main.js"), "utf8");

const failures = [];

function check(name, ok, detail = "") {
  if (ok) {
    console.log(`  ok   ${name}`);
  } else {
    console.log(`  FAIL ${name}${detail ? `: ${detail}` : ""}`);
    failures.push(name);
  }
}

/// All values of an HTML attribute, e.g. every class= in the markup.
function attrValues(attr) {
  const found = new Set();
  for (const m of html.matchAll(new RegExp(`${attr}="([^"]+)"`, "g"))) {
    for (const value of m[1].trim().split(/\s+/)) found.add(value);
  }
  return found;
}

const htmlClasses = attrValues("class");
const htmlIds = attrValues("id");

// Selectors the stylesheet defines, ignoring anything inside a comment — the
// comment explaining the tread→ticks rename mentions the old name, and a naive
// scan would happily accept it as still defined.
// Declarations go too, not just comments: `stroke: #c93b3b` would otherwise
// scan as an id selector named c93b3b. Selectors live outside declarations, so
// dropping every `property: value;` leaves exactly what we want to check.
const cssSelectorsOnly = css
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/[\w-]+\s*:\s*[^;{}]+;/g, "");

const cssClasses = new Set([...cssSelectorsOnly.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));
const cssIds = new Set([...cssSelectorsOnly.matchAll(/#([a-zA-Z][\w-]*)/g)].map((m) => m[1]));

console.log("test_markup_classes_are_styled");
for (const cls of [...htmlClasses].sort()) {
  check(`.${cls} has a rule`, cssClasses.has(cls), "used in index.html, absent from styles.css");
}

console.log("test_stylesheet_targets_exist_in_markup");
// The other direction: a rule for a class nobody uses is dead weight, and
// usually the wreckage of a half-finished rename.
for (const cls of [...cssClasses].sort()) {
  check(`.${cls} is used`, htmlClasses.has(cls), "styled in styles.css, absent from index.html");
}
for (const id of [...cssIds].sort()) {
  check(`#${id} is used`, htmlIds.has(id), "styled in styles.css, absent from index.html");
}

console.log("test_script_selectors_resolve");
// querySelector("#reading") and friends — the ones that would throw at runtime.
const queried = new Set([...js.matchAll(/querySelector\(\s*"#([\w-]+)"/g)].map((m) => m[1]));
for (const id of [...queried].sort()) {
  check(`querySelector("#${id}") resolves`, htmlIds.has(id), "queried in main.js, absent from index.html");
}
check("the script queries something at all", queried.size > 0, "regex found no selectors — has main.js changed shape?");

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nall passed");
