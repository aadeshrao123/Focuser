// Tests for the extension's hostname matching.
//
// `background.js` is a service worker that touches `chrome.*` as soon as it
// loads, so it cannot simply be imported. The canonicalisation block is fenced
// by the two comment banners below and evaluated on its own — the same source
// that ships, without the browser around it.
//
// Run with: node --test extension/
//
// These rules must stay in step with `focuser_common::host`. If you change one
// side, change the other, or which form the user typed starts to matter again.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const source = readFileSync(fileURLToPath(new URL("background.js", import.meta.url)), "utf8");
const start = source.indexOf("function canonicalHost");
const end = source.indexOf("// ─── Domain Matching");
assert.ok(start !== -1 && end > start, "could not find the canonicalisation block");

const { canonicalHost, canonicalSet, setCovers } = new Function(
  `${source.slice(start, end)}; return { canonicalHost, canonicalSet, setCovers };`,
)();

test("canonicalHost strips everything that is not the host", () => {
  for (const raw of [
    "youtube.com",
    "  YouTube.com  ",
    "www.youtube.com",
    "WWW.YOUTUBE.COM",
    "https://www.youtube.com",
    "http://youtube.com/feed/subscriptions",
    "https://www.youtube.com:443/watch?v=abc#t=10",
    "youtube.com.",
    "https://user:pass@www.youtube.com/",
  ]) {
    assert.equal(canonicalHost(raw), "youtube.com", `input was ${raw}`);
  }
});

test("canonicalHost leaves a deeper subdomain alone", () => {
  assert.equal(canonicalHost("music.youtube.com"), "music.youtube.com");
  assert.equal(canonicalHost("www.music.youtube.com"), "music.youtube.com");
});

test("www is irrelevant in every direction", () => {
  for (const rule of ["youtube.com", "www.youtube.com"]) {
    const set = canonicalSet([rule]);
    for (const host of ["youtube.com", "www.youtube.com", "WWW.YouTube.com"]) {
      assert.ok(setCovers(set, host), `${rule} should cover ${host}`);
    }
  }
});

test("a rule covers its subdomains but not its parent", () => {
  const set = canonicalSet(["youtube.com"]);
  assert.ok(setCovers(set, "music.youtube.com"));
  assert.ok(setCovers(set, "m.youtube.com"));

  const deep = canonicalSet(["music.youtube.com"]);
  assert.equal(setCovers(deep, "youtube.com"), false);
});

test("suffixes have to land on a label boundary", () => {
  const set = canonicalSet(["youtube.com"]);
  assert.equal(setCovers(set, "notyoutube.com"), false);
  assert.equal(setCovers(canonicalSet(["tube.com"]), "youtube.com"), false);
});

test("blank entries never match anything", () => {
  const set = canonicalSet(["", "   ", null, undefined]);
  assert.equal(set.size, 0);
  assert.equal(setCovers(set, "youtube.com"), false);
  assert.equal(setCovers(canonicalSet(["youtube.com"]), ""), false);
});

test("a full URL is matched by its host", () => {
  const set = canonicalSet(["youtube.com"]);
  assert.ok(setCovers(set, "https://www.youtube.com/watch?v=abc"));
});
