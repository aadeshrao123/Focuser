import { describe, expect, it } from "vitest";
import { assignColors, colorFor, gradientId, SERIES_COLORS } from "./chart-colors";

describe("colorFor", () => {
  it("gives a target the same colour every time", () => {
    // The whole point: a site keeps its colour across ranges and sessions.
    expect(colorFor("reddit.com")).toBe(colorFor("reddit.com"));
  });

  it("does not depend on what else is on screen", () => {
    // Colour follows the entity, not its rank, so filtering the list must not
    // repaint the targets that survive.
    const before = ["reddit.com", "youtube.com", "x.com"].map(colorFor);
    const after = ["youtube.com", "reddit.com"].map(colorFor);
    expect(after[0]).toBe(before[1]);
    expect(after[1]).toBe(before[0]);
  });

  it("only ever returns a colour from the validated palette", () => {
    const names = Array.from({ length: 500 }, (_, i) => `site-${i}.example`);
    for (const name of names) {
      expect(SERIES_COLORS).toContain(colorFor(name));
    }
  });

  it("spreads across the palette rather than favouring one slot", () => {
    const counts = new Map<string, number>();
    for (let i = 0; i < 500; i++) {
      const c = colorFor(`site-${i}.example`);
      counts.set(c, (counts.get(c) ?? 0) + 1);
    }
    expect(counts.size).toBe(SERIES_COLORS.length);
    // Even-ish: no slot takes more than double its fair share.
    for (const n of counts.values()) expect(n).toBeLessThan((500 / SERIES_COLORS.length) * 2);
  });

  it("spreads real domain names, not just synthetic ones", () => {
    // The regression this pins: without an avalanche step on the hash, FNV's
    // low bits correlate across similar strings and 10 of these 18 landed on
    // the same colour.
    const sites = [
      "reddit.com",
      "youtube.com",
      "x.com",
      "news.ycombinator.com",
      "instagram.com",
      "twitch.tv",
      "tiktok.com",
      "facebook.com",
      "netflix.com",
      "discord.com",
      "twitter.com",
      "pinterest.com",
      "linkedin.com",
      "amazon.com",
      "ebay.com",
      "steam.exe",
      "discord.exe",
      "spotify.com",
    ];
    const counts = new Map<string, number>();
    for (const s of sites) counts.set(colorFor(s), (counts.get(colorFor(s)) ?? 0) + 1);

    expect(counts.size).toBe(SERIES_COLORS.length);
    expect(Math.max(...counts.values())).toBeLessThanOrEqual(4);
  });

  it("handles an empty name without throwing", () => {
    expect(SERIES_COLORS).toContain(colorFor(""));
  });
});

describe("assignColors", () => {
  it("never gives two targets in one chart the same colour", () => {
    const targets = ["youtube.com", "news.ycombinator.com", "instagram.com", "a.com", "b.com"];
    const colors = assignColors(targets);
    expect(new Set(colors.values()).size).toBe(targets.length);
  });

  it("keeps a target's own colour when nothing else wants it", () => {
    const only = assignColors(["reddit.com"]);
    expect(only.get("reddit.com")).toBe(colorFor("reddit.com"));
  });

  it("wraps rather than running out past the palette", () => {
    const many = Array.from({ length: 12 }, (_, i) => `site-${i}.example`);
    const colors = assignColors(many);
    expect(colors.size).toBe(many.length);
    for (const c of colors.values()) expect(SERIES_COLORS).toContain(c);
  });
});

describe("gradientId", () => {
  it("is stable and safe to put in an id attribute", () => {
    expect(gradientId("reddit.com")).toBe(gradientId("reddit.com"));
    expect(gradientId("a b/c#d.com")).toMatch(/^series-[a-z0-9]+$/);
  });

  it("separates different targets", () => {
    expect(gradientId("reddit.com")).not.toBe(gradientId("youtube.com"));
  });
});
