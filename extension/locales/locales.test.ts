import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { load } from "js-yaml";
import { describe, expect, it } from "vitest";

/**
 * Checks every translation against English.
 *
 * `tsc` already refuses a key that does not exist in the default locale, so
 * what it cannot see is the *other* files: a missing key falls back to English
 * silently, and a dropped `{placeholder}` renders a sentence with a hole in it.
 *
 * Note this catalogue is separate from the desktop app's. The extension follows
 * the browser's language and the app follows its own setting, so the two are
 * chosen independently and share nothing but a tone of voice.
 */

const DIR = "locales";
const BASE = "en";

type Node = string | number | { [key: string]: Node };

/** `{ block: { goBack: "..." } }` becomes `{ "block.goBack": "..." }`. */
function flatten(node: Node, prefix = ""): Record<string, string> {
  if (typeof node !== "object" || node === null) return { [prefix]: String(node) };

  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(node)) {
    const path = prefix ? `${prefix}.${key}` : key;
    Object.assign(out, flatten(value as Node, path));
  }
  return out;
}

function locale(name: string): Record<string, string> {
  const raw = readFileSync(join(DIR, `${name}.yml`), "utf8");
  return flatten(load(raw) as Node);
}

/** `{name}` named substitutions and `$1` positional ones. */
function placeholders(message: string): string[] {
  const named = [...message.matchAll(/\{(\w+)\}/g)].map((m) => m[1] as string);
  const positional = [...message.matchAll(/(?<!\$)\$(\d)/g)].map((m) => `$${m[1]}`);
  return [...named, ...positional].sort();
}

const names = readdirSync(DIR)
  .filter((f) => f.endsWith(".yml"))
  .map((f) => f.replace(/\.yml$/, ""));

const base = locale(BASE);

/**
 * Chrome's `_locales` list, which is not the same as BCP 47.
 *
 * There is no plain `pt` or `zh` on it — a `_locales/pt/` directory is silently
 * ignored and a Brazilian user gets English. WXT warns about this during
 * `prepare` and the warning is easy to scroll past, so it is a test too.
 *
 * From https://developer.chrome.com/docs/extensions/reference/api/i18n#locales
 */
const SUPPORTED = new Set(
  `ar am bg bn ca cs da de el en en_AU en_GB en_US es es_419 et fa fi fil fr gu he hi hr hu
   id it ja kn ko lt lv ml mr ms nl no pl pt_BR pt_PT ro ru sk sl sr sv sw ta te th tr uk vi
   zh_CN zh_TW`.split(/\s+/),
);

describe("extension locales", () => {
  it("has English with messages in it", () => {
    expect(names).toContain(BASE);
    expect(Object.keys(base).length).toBeGreaterThan(30);
  });

  it("uses locale codes browsers actually recognise", () => {
    expect(names.filter((n) => !SUPPORTED.has(n))).toEqual([]);
  });

  describe.each(names.filter((n) => n !== BASE))("%s", (name) => {
    const translated = locale(name);

    it("has exactly the keys English has", () => {
      expect(Object.keys(translated).sort()).toEqual(Object.keys(base).sort());
    });

    it("keeps every placeholder", () => {
      for (const key of Object.keys(base)) {
        expect(placeholders(translated[key] ?? ""), `${key} in ${name}`).toEqual(
          placeholders(base[key] as string),
        );
      }
    });

    it("keeps plural forms as plural forms", () => {
      // A plural is a `1:`/`n:` pair, which flattens to `key.1` and `key.n`.
      const pluralKeys = Object.keys(base).filter((k) => k.endsWith(".n"));
      expect(pluralKeys.length).toBeGreaterThan(0);
      for (const key of pluralKeys) {
        expect(translated[key], `${key} in ${name}`).toBeDefined();
        expect(translated[key.replace(/\.n$/, ".1")], `${key} in ${name}`).toBeDefined();
      }
    });

    it("translates something", () => {
      const identical = Object.keys(base).filter((k) => base[k] === translated[k]);
      expect(identical.length).toBeLessThan(Object.keys(base).length * 0.5);
    });
  });
});
