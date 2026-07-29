import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Every user-visible string in the extension has to come from `locales/en.yml`.
 *
 * The desktop app has had this check since it was translated; the extension did
 * not, and the word "Blocked" — the largest, most-read word on the block page —
 * shipped in English in every language because of it. A missing key is a build
 * error, but a string that was never a key is invisible.
 *
 * Deliberately a regex rather than a parser, so it over-reports rather than
 * under-reports. `ALLOWED` is the escape hatch and every entry is a decision.
 */

const ROOTS = ["components", "entrypoints"];

/** Props whose value a user reads. */
const TEXT_PROPS = ["title", "label", "placeholder", "aria-label", "alt"];

const ALLOWED = new Set([
  // The product name, which is not translated.
  "Focuser",
  // What is printed on the key. Every keyboard says "Esc".
  "Esc",
]);

/** Type syntax the `>` heuristic mistakes for text: `Props> extends Foo<`. */
const TYPE_SYNTAX = /^(extends|implements|keyof|typeof|infer|readonly)\b/;

function walk(path: string): string[] {
  if (statSync(path).isFile()) return path.endsWith(".tsx") ? [path] : [];
  return readdirSync(path).flatMap((entry) => walk(join(path, entry)));
}

/**
 * Text between tags: `>Some words<`.
 *
 * `>` also ends an arrow function and a comparison, so anything carrying
 * operator or call punctuation is rejected — real UI text has no `=` or `(`.
 */
function jsxTextNodes(source: string): string[] {
  const found: string[] = [];
  // The second pattern is text that runs into an interpolation, as in
  // `>Version {VERSION}<`. Stopping at `<` alone misses it entirely.
  const patterns = [/>\s*([A-Za-z][^<>{}]*?)\s*</g, />\s*([A-Za-z][^<>{}]*?)\s*\{/g];
  for (const match of patterns.flatMap((p) => [...source.matchAll(p)])) {
    const text = match[1]?.replace(/\s+/g, " ").trim();
    if (!text) continue;
    if (/[=;()[\]`$\\]/.test(text)) continue;
    if (TYPE_SYNTAX.test(text)) continue;
    if (!/[A-Za-z]{2}/.test(text)) continue;
    // A lone lowercase word is almost always an identifier or a unit.
    if (/^[a-z]+$/.test(text)) continue;
    found.push(text);
  }
  return found;
}

/**
 * `{connected ? "On" : "Off"}` — text inside a JSX expression, which the `>…<`
 * scan cannot see because the braces hide it. This exact line shipped
 * untranslated in ten languages.
 */
function ternaryLiterals(source: string): string[] {
  const found: string[] = [];
  for (const match of source.matchAll(/\?\s*"([^"]{2,})"\s*:\s*"([^"]{2,})"/g)) {
    for (const value of [match[1], match[2]]) {
      // Tailwind class strings are the other thing shaped like this.
      if (value && /^[A-Z]/.test(value) && !value.includes("-")) found.push(value);
    }
  }
  return found;
}

/** `title="Some words"` — a literal where a message should be. */
function literalProps(source: string): string[] {
  const found: string[] = [];
  const pattern = new RegExp(`\\b(${TEXT_PROPS.join("|")})=\\{?"([^"]{2,})"\\}?`, "g");
  for (const match of source.matchAll(pattern)) {
    const value = match[2];
    if (value && /[A-Za-z]{2}/.test(value)) found.push(value);
  }
  return found;
}

describe("user-facing strings come from the catalogue", () => {
  const files = ROOTS.flatMap(walk).filter((f) => !f.endsWith(".test.tsx"));

  it("finds files to check", () => {
    expect(files.length).toBeGreaterThan(2);
  });

  it.each(files)("%s", (file) => {
    const source = readFileSync(file, "utf8");
    const offenders = [...jsxTextNodes(source), ...literalProps(source), ...ternaryLiterals(source)]
      .map((s) => s.trim())
      .filter((s) => !ALLOWED.has(s));

    expect(offenders, `hardcoded text in ${file} — move it into locales/en.yml`).toEqual([]);
  });
});
