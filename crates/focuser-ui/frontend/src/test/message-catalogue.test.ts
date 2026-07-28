import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Checks every translation against English.
 *
 * A translator working in a text editor cannot be expected to notice a dropped
 * `{count}` or a key that quietly went missing, and neither of those shows up
 * until someone is looking at the broken screen. These run on every commit so
 * the answer arrives in seconds instead.
 *
 * Paraglide already makes a *missing* key a build error. What it cannot see is
 * a key present but wrong.
 */

const DIR = "messages";
const BASE = "en";

type Message = string | Variant[];
interface Variant {
  declarations?: string[];
  selectors?: string[];
  match?: Record<string, string>;
}

function load(locale: string): Record<string, Message> {
  const raw = JSON.parse(readFileSync(join(DIR, `${locale}.json`), "utf8"));
  delete raw.$schema;
  return raw;
}

/** Every `{placeholder}` a message will substitute, in any of its variants. */
function placeholders(message: Message): Set<string> {
  const text = typeof message === "string" ? message : JSON.stringify(message);
  return new Set([...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1] as string));
}

/** Rendered forms of a message — one per variant, or the string itself. */
function forms(message: Message): string[] {
  if (typeof message === "string") return [message];
  return message.flatMap((variant) => Object.values(variant.match ?? {}));
}

const locales = readdirSync(DIR)
  .filter((f) => f.endsWith(".json"))
  .map((f) => f.replace(/\.json$/, ""));

const base = load(BASE);

describe("message catalogue", () => {
  it("has English", () => {
    expect(locales).toContain(BASE);
    expect(Object.keys(base).length).toBeGreaterThan(50);
  });

  it("leaves no placeholder unclosed in English", () => {
    for (const [key, message] of Object.entries(base)) {
      for (const form of forms(message)) {
        // A stray brace means a substitution that will render literally.
        const braces = (form.match(/[{}]/g) ?? []).length;
        expect(braces % 2, `${key} has an unbalanced brace: ${form}`).toBe(0);
      }
    }
  });

  describe.each(locales.filter((l) => l !== BASE))("%s", (locale) => {
    const translated = load(locale);

    it("has exactly the keys English has", () => {
      expect(Object.keys(translated).sort()).toEqual(Object.keys(base).sort());
    });

    it("keeps every placeholder", () => {
      for (const key of Object.keys(base)) {
        const expected = [...placeholders(base[key] as Message)].sort();
        const actual = [...placeholders(translated[key] as Message)].sort();
        expect(actual, `${key} in ${locale} lost or invented a placeholder`).toEqual(expected);
      }
    });

    it("keeps plurals as plurals", () => {
      for (const key of Object.keys(base)) {
        const baseIsVariant = Array.isArray(base[key]);
        const thisIsVariant = Array.isArray(translated[key]);
        expect(thisIsVariant, `${key} in ${locale} must stay a variant block`).toBe(baseIsVariant);
      }
    });

    it("translates something", () => {
      // A copy of English is not a translation. Allow a few, since some words
      // genuinely do not change, but not the whole file.
      const identical = Object.keys(base).filter(
        (k) => JSON.stringify(base[k]) === JSON.stringify(translated[k]),
      );
      expect(identical.length).toBeLessThan(Object.keys(base).length * 0.5);
    });
  });
});
