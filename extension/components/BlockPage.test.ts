import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { categoryInfo } from "@/lib/categories";
import { ICONS, ordinal } from "./BlockPage";

/** The real file the extension ships, not a fixture. */
const premade = JSON.parse(
  readFileSync(
    resolve(__dirname, "../../crates/focuser-ui/frontend/public/premade-lists.json"),
    "utf8",
  ),
) as { categories: Record<string, unknown> };

describe("ordinal", () => {
  it("handles the teens, which do not follow the last-digit rule", () => {
    expect(ordinal(11)).toBe("11th");
    expect(ordinal(12)).toBe("12th");
    expect(ordinal(13)).toBe("13th");
    expect(ordinal(111)).toBe("111th");
    expect(ordinal(112)).toBe("112th");
  });

  it("uses the last digit everywhere else", () => {
    expect(ordinal(1)).toBe("1st");
    expect(ordinal(2)).toBe("2nd");
    expect(ordinal(3)).toBe("3rd");
    expect(ordinal(4)).toBe("4th");
    expect(ordinal(21)).toBe("21st");
    expect(ordinal(22)).toBe("22nd");
    expect(ordinal(103)).toBe("103rd");
    expect(ordinal(137)).toBe("137th");
  });
});

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
