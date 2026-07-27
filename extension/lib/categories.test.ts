import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  buildIndex,
  CATEGORIES,
  categoryForHost,
  categoryForKeyword,
  categoryInfo,
  DEFAULT_CATEGORY,
  messageFor,
  resolveCategory,
} from "./categories";

/** The real file the extension ships, not a fixture. */
const premade = JSON.parse(
  readFileSync(
    resolve(__dirname, "../../crates/focuser-ui/frontend/public/premade-lists.json"),
    "utf8",
  ),
) as { categories: Record<string, { domains?: string[] }> };

describe("resolveCategory", () => {
  it("maps every category id in the shipped starter lists to a message set", () => {
    // The regression this pins: the file key is `videos` while the alias map
    // only had `video`, so the largest category fell back to generic text and
    // nothing failed loudly.
    const unmapped = Object.keys(premade.categories).filter(
      (id) => resolveCategory(id) === DEFAULT_CATEGORY,
    );
    expect(unmapped, `unmapped category ids: ${unmapped.join(", ")}`).toEqual([
      // These have no distinct voice of their own and are meant to fall back.
      "email",
      "proxies",
      "search_engines",
      "productivity",
    ]);
  });

  it("resolves aliases and is case-insensitive", () => {
    expect(resolveCategory("porn")).toBe("adult");
    expect(resolveCategory("NSFW")).toBe("adult");
    expect(resolveCategory("videos")).toBe("video");
    expect(resolveCategory("games")).toBe("gaming");
  });

  it("falls back rather than inventing a category", () => {
    expect(resolveCategory("nonsense")).toBe(DEFAULT_CATEGORY);
    expect(resolveCategory("")).toBe(DEFAULT_CATEGORY);
  });
});

describe("buildIndex", () => {
  const index = buildIndex(premade);

  it("indexes every domain in the shipped file", () => {
    const total = Object.values(premade.categories).reduce(
      (n, c) => n + (c.domains?.length ?? 0),
      0,
    );
    // Allows for www-duplicates collapsing, but catches the file being missed.
    expect(Object.keys(index.hosts).length).toBeGreaterThan(total * 0.9);
  });

  it("files known sites under the right category", () => {
    expect(categoryForHost(index, "tinder.com")).toBe("dating");
    expect(categoryForHost(index, "youtube.com")).toBe("video");
  });

  it("matches parent domains", () => {
    expect(categoryForHost(index, "www.tinder.com")).toBe("dating");
    expect(categoryForHost(index, "some.sub.tinder.com")).toBe("dating");
  });

  it("returns the default for an unknown host", () => {
    expect(categoryForHost(index, "example-nothing.test")).toBe(DEFAULT_CATEGORY);
  });

  it("survives a malformed file instead of throwing", () => {
    expect(buildIndex(null).hosts).toEqual({});
    expect(buildIndex({ categories: null }).hosts).toEqual({});
    expect(buildIndex("nonsense").hosts).toEqual({});
  });
});

describe("categoryForKeyword", () => {
  it("guesses a category from a keyword that appears in known hosts", () => {
    const index = buildIndex({
      categories: { gambling: { domains: ["bigcasino.test"] } },
    });
    expect(categoryForKeyword(index, "casino")).toBe("gambling");
  });

  it("falls back when nothing resembles the keyword", () => {
    expect(categoryForKeyword(buildIndex(premade), "zzzznothing")).toBe(DEFAULT_CATEGORY);
  });
});

describe("messageFor", () => {
  it("escalates as the count rises", () => {
    // A single fixed string stops being read after the second visit; the
    // tiers are the reason the page keeps working.
    const first = messageFor("social_media", 1);
    const persistent = messageFor("social_media", 12);
    expect(first).not.toBe(persistent);
    expect(first.length).toBeGreaterThan(0);
    expect(persistent.length).toBeGreaterThan(0);
  });

  it("is stable for the same category and count", () => {
    // A re-render must not shuffle the text under the reader.
    expect(messageFor("gaming", 5)).toBe(messageFor("gaming", 5));
  });

  it("always returns something, for any category and any count", () => {
    for (const category of ["adult", "unknown", "", "video"]) {
      for (const count of [0, 1, 3, 50, 100_000]) {
        expect(messageFor(category, count).length, `${category}/${count}`).toBeGreaterThan(0);
      }
    }
  });

  it("never leaves a placeholder on screen", () => {
    // Roughly 40% of the shipped messages contain {count} or {domain}. An
    // unsubstituted one reaches the user as literal braces.
    for (const category of Object.keys(CATEGORIES)) {
      for (let count = 1; count <= 60; count++) {
        const text = messageFor(category, count, "example.com");
        expect(text, `${category} @ ${count}`).not.toMatch(/\{\w+\}/);
      }
    }
  });

  it("substitutes the count and the domain", () => {
    const withBoth = Object.values(CATEGORIES)
      .flatMap((c) => c.tiers.flatMap((t) => t.messages))
      .some((m) => m.includes("{count}") && m.includes("{domain}"));
    expect(withBoth, "the data should still exercise both placeholders").toBe(true);

    // Find a count whose chosen message uses {domain} and check it landed.
    const rendered = Array.from({ length: 40 }, (_, i) =>
      messageFor("social_media", i + 1, "reddit.com"),
    );
    expect(rendered.some((m) => m.includes("reddit.com"))).toBe(true);
  });

  it("falls back to a readable phrase when there is no domain", () => {
    const rendered = Array.from({ length: 40 }, (_, i) => messageFor("social_media", i + 1));
    expect(rendered.every((m) => !m.includes("{"))).toBe(true);
  });
});

describe("categoryInfo", () => {
  it("carries a label and a colour for the block page", () => {
    const info = categoryInfo("adult");
    expect(info.label.length).toBeGreaterThan(0);
    expect(info.color).toMatch(/^#[0-9a-f]{6}$/i);
    expect(info.tiers.length).toBeGreaterThan(0);
  });
});
