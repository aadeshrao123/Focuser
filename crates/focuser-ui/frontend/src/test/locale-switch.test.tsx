import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { m } from "@/paraglide/messages.js";
import { baseLocale, getLocale, locales, setLocale } from "@/paraglide/runtime.js";

/**
 * The switch itself, end to end.
 *
 * The catalogue tests prove the files agree with each other. This proves the
 * app actually renders the other language, which is the part a user notices.
 */

function Heading() {
  return <h1>{m.websites_title()}</h1>;
}

describe("switching language", () => {
  beforeEach(() => setLocale(baseLocale, { reload: false }));

  it("ships more than one language", () => {
    expect(locales.length).toBeGreaterThan(1);
    expect(locales).toContain("en");
  });

  it("starts in English", () => {
    expect(getLocale()).toBe("en");
    expect(m.websites_title()).toBe("Websites");
  });

  it("changes what a component renders", () => {
    const { rerender } = render(<Heading />);
    expect(screen.getByRole("heading")).toHaveTextContent("Websites");

    setLocale("es", { reload: false });
    rerender(<Heading />);

    expect(screen.getByRole("heading")).toHaveTextContent("Sitios web");
  });

  it("substitutes placeholders in the new language", () => {
    setLocale("es", { reload: false });
    const text = m.dashboard_enable_list({ name: "Deep work" });

    expect(text).toContain("Deep work");
    expect(text).not.toContain("{name}");
    expect(text).not.toBe(m.dashboard_enable_list({ name: "Deep work" }, { locale: "en" }));
  });

  it("picks the right plural form per language", () => {
    setLocale("en", { reload: false });
    expect(m.count_sites({ count: 1 })).toBe("1 site");
    expect(m.count_sites({ count: 4 })).toBe("4 sites");

    setLocale("es", { reload: false });
    expect(m.count_sites({ count: 1 })).toBe("1 sitio");
    expect(m.count_sites({ count: 4 })).toBe("4 sitios");
  });

  /**
   * Every plural category the language actually has, not just one and two.
   *
   * Spanish shipped broken because of this: CLDR gives it a `many` category for
   * large numbers, the catalogue only had `one` and `other`, and
   * `count_sites({ count: 1000000 })` rendered the literal string
   * "count_sites". Testing with 1 and 4 never went near it.
   */
  it.each(locales)("covers every plural category in %s", (locale) => {
    setLocale(locale, { reload: false });
    const rules = new Intl.PluralRules(locale);

    // One number per category this language distinguishes.
    const samples = new Map<string, number>();
    for (const n of [0, 1, 2, 3, 5, 11, 21, 100, 1000, 1_000_000]) {
      const category = rules.select(n);
      if (!samples.has(category)) samples.set(category, n);
    }
    expect(samples.size).toBeGreaterThan(0);

    for (const [name, message] of Object.entries(m)) {
      if (typeof message !== "function") continue;
      for (const [category, n] of samples) {
        const rendered = message({
          count: n,
          name: "x",
          total: n,
          enabled: 1,
          visible: 1,
          noun: "x",
          version: "x",
          phase: "x",
          minutes: 1,
          time: "x",
          hour: "x",
          day: "x",
          path: "x",
          lists: "x",
          duration: "x",
          query: "x",
          target: "x",
          limit: "x",
          used: "x",
          left: "x",
          example: "x",
          added: "x",
          alreadyThere: "x",
        });
        expect(rendered, `${name} in ${locale} has no "${category}" form`).not.toBe(name);
      }
    }
  });

  it("renders no message as a bare key or an empty string", () => {
    // A key leaking through means a variant matched nothing.
    for (const locale of locales) {
      setLocale(locale, { reload: false });
      for (const [name, message] of Object.entries(m)) {
        if (typeof message !== "function") continue;
        const rendered = message({
          count: 2,
          name: "x",
          total: 2,
          enabled: 1,
          visible: 1,
          noun: "x",
          version: "x",
          phase: "x",
          minutes: 1,
          time: "x",
          hour: "x",
          day: "x",
          path: "x",
          lists: "x",
          duration: "x",
          query: "x",
          target: "x",
          limit: "x",
          used: "x",
          left: "x",
          example: "x",
          added: "x",
          alreadyThere: "x",
        });
        expect(rendered, `${name} in ${locale}`).not.toBe(name);
        expect(String(rendered).trim(), `${name} in ${locale}`).not.toBe("");
      }
    }
  });
});
