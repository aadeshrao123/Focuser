import { describe, expect, it } from "vitest";
import { cn, count } from "./utils";

describe("cn", () => {
  it("keeps the last of two conflicting Tailwind utilities", () => {
    expect(cn("p-2", "p-4")).toBe("p-4");
  });

  it("drops falsy values", () => {
    expect(cn("a", false && "b", undefined, "c")).toBe("a c");
  });
});

describe("count", () => {
  it("uses the singular for exactly one", () => {
    expect(count(1, "site")).toBe("1 site");
  });

  it.each([0, 2, 17])("pluralises %i", (n) => {
    expect(count(n, "site")).toBe(`${n} sites`);
  });

  it("takes an explicit plural for irregular words", () => {
    expect(count(3, "entry", "entries")).toBe("3 entries");
  });
});
