import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Every user-visible string has to come from the message catalogue.
 *
 * Without this, the app drifts back to hardcoded English one pull request at a
 * time: someone adds a page, ships it, and nobody notices until a translator
 * asks why half the screen is untranslated. A test is the only thing that keeps
 * that from happening quietly.
 *
 * It is deliberately dumb — a regex, not a parser — so it over-reports rather
 * than under-reports. `ALLOWED` is the escape hatch, and every entry in it is a
 * decision someone made on purpose.
 */

const ROOTS = ["src/routes", "src/components", "src/app-layout.tsx"];

/** Props whose value a user reads. */
const TEXT_PROPS = [
  "title",
  "description",
  "label",
  "placeholder",
  "aria-label",
  "content",
  "hint",
  "confirmLabel",
  "noun",
  "emptyLabel",
];

/**
 * Strings that look like prose but are not.
 *
 * Keep this short and keep the reasons obvious. A long allowlist means the
 * check has stopped being useful.
 */
const ALLOWED = new Set([
  // Example values in placeholders. Deliberately not translated: a domain and a
  // file path look the same in every language, and translating `discord.exe`
  // would invite a rule that matches nothing.
  "reddit.com",
  "docs.example.com",
  "casino",
  "*.reddit.com",
  "/r/gaming",
  "discord.exe",
  "C:\\Program Files\\Steam\\steam.exe",
  "Discord",
  "/Applications/Steam.app",
  "discord",
  "/usr/bin/steam",
  "Solitaire",
  // Sample input for the bulk-import box. Domains and a `#` comment read the
  // same in every language.
  "reddit.com\\ntwitter.com\\n# social\\nfacebook.com",
  // The product name, which is not translated.
  "Focuser",
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
 * `>` also ends an arrow function and a comparison, so a naive match reads
 * ordinary code as prose. Anything carrying operator or call punctuation is
 * therefore rejected — real UI text does not contain `=`, `(` or `;`.
 */
function jsxTextNodes(source: string): string[] {
  const found: string[] = [];
  // The second pattern is text that runs into an interpolation, as in
  // `>Version {version}<`. Stopping at `<` alone misses it entirely; that is
  // how the extension shipped an untranslated version badge.
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
 * scan cannot see because the braces hide it.
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

/**
 * ``aria-label={`${n} cycles`}`` — a template literal where a message should
 * be. Only the static chunks are looked at; the `${…}` parts are values.
 *
 * Any English word counts here, including a lowercase one. Elsewhere a bare
 * lowercase word is usually an identifier, but the whole point of a label prop
 * is that a person reads it.
 */
function templateProps(source: string): string[] {
  const found: string[] = [];
  const pattern = new RegExp(`\\b(${TEXT_PROPS.join("|")})=\\{\`([^\`]+)\`\\}`, "g");
  for (const match of source.matchAll(pattern)) {
    for (const chunk of (match[2] ?? "").split(/\$\{[^}]*\}/)) {
      const text = chunk.replace(/\s+/g, " ").trim();
      if (text && /[A-Za-z]{2}/.test(text)) found.push(text);
    }
  }
  return found;
}

/**
 * Any template literal carrying a capitalised word, wherever it sits.
 * `{v ? \`Version ${v} available\` : "…"}` is how the update badge shipped
 * untranslated. Tailwind class strings are lowercase, so they do not trip it.
 */
function templateProse(source: string): string[] {
  const found: string[] = [];
  for (const match of source.matchAll(/`([^`]*\$\{[^`]*)`/g)) {
    for (const chunk of (match[1] ?? "").split(/\$\{[^}]*\}/)) {
      const text = chunk.replace(/\s+/g, " ").trim();
      if (text && /\b[A-Z][a-z]{2,}/.test(text)) found.push(text);
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
    expect(files.length).toBeGreaterThan(10);
  });

  it.each(files)("%s", (file) => {
    const source = readFileSync(file, "utf8");
    const offenders = [
      ...jsxTextNodes(source),
      ...literalProps(source),
      ...ternaryLiterals(source),
      ...templateProps(source),
      ...templateProse(source),
    ]
      .map((s) => s.trim())
      .filter((s) => !ALLOWED.has(s));

    expect(offenders, `hardcoded text in ${file} — move it into messages/en.json`).toEqual([]);
  });
});
