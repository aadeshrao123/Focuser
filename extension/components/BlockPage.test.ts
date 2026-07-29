import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { categoryInfo } from "@/lib/categories";
import { ICONS } from "./BlockPage";

/** The real file the extension ships, not a fixture. */
const premade = JSON.parse(
  readFileSync(
    // The extension's own copy. Reaching across to `crates/` is what made the
    // sources archive unbuildable for an AMO reviewer.
    resolve(__dirname, "../public/premade-lists.json"),
    "utf8",
  ),
) as { categories: Record<string, unknown> };

describe("ICONS", () => {
  it("has a glyph for every category the shipped lists can resolve to", () => {
    // Same shape of bug as the unmapped `videos` category: without this, a new
    // category silently renders the generic shield and nothing fails.
    const missing = Object.keys(premade.categories)
      .map((id) => categoryInfo(id).icon)
      .filter((icon) => !(icon in ICONS));
    expect(missing, `category icons with no component: ${[...new Set(missing)].join(", ")}`).toEqual(
      [],
    );
  });

  it("covers every icon named in the message data", () => {
    const messages = JSON.parse(
      readFileSync(resolve(__dirname, "../lib/category-messages.json"), "utf8"),
    ) as { categories: Record<string, { icon?: string }> };
    for (const [id, info] of Object.entries(messages.categories)) {
      expect(info.icon && info.icon in ICONS, `${id} → ${info.icon}`).toBe(true);
    }
  });
});
