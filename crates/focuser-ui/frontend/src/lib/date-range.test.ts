import { describe, expect, it } from "vitest";
import { daysBetween, formatDay, parseIsoDate, rangeFor, toIsoDate } from "./date-range";

describe("toIsoDate", () => {
  it("uses local calendar fields, not UTC", () => {
    // Late evening: toISOString() would report the next day in any positive offset.
    expect(toIsoDate(new Date(2026, 6, 27, 23, 30))).toBe("2026-07-27");
  });

  it("pads single-digit months and days", () => {
    expect(toIsoDate(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});

describe("rangeFor", () => {
  const today = new Date(2026, 6, 27);

  it("treats today as a one-day range", () => {
    expect(rangeFor("today", today)).toEqual({ from: "2026-07-27", to: "2026-07-27" });
  });

  it("counts both ends of a seven-day range", () => {
    expect(rangeFor("7d", today)).toEqual({ from: "2026-07-21", to: "2026-07-27" });
  });

  it("crosses month boundaries", () => {
    expect(rangeFor("30d", today).from).toBe("2026-06-28");
  });
});

describe("daysBetween", () => {
  it("includes both ends", () => {
    expect(daysBetween({ from: "2026-07-25", to: "2026-07-27" })).toEqual([
      "2026-07-25",
      "2026-07-26",
      "2026-07-27",
    ]);
  });

  it("returns a single day when the ends match", () => {
    expect(daysBetween({ from: "2026-07-27", to: "2026-07-27" })).toEqual(["2026-07-27"]);
  });

  it("returns nothing when the range is inverted", () => {
    expect(daysBetween({ from: "2026-07-27", to: "2026-07-25" })).toEqual([]);
  });

  it("spans a leap day", () => {
    expect(daysBetween({ from: "2028-02-28", to: "2028-03-01" })).toEqual([
      "2028-02-28",
      "2028-02-29",
      "2028-03-01",
    ]);
  });
});

describe("parseIsoDate", () => {
  it("round-trips through toIsoDate", () => {
    expect(toIsoDate(parseIsoDate("2026-12-31"))).toBe("2026-12-31");
  });
});

describe("formatDay", () => {
  it("drops the leading zero from the day", () => {
    expect(formatDay("2026-01-05")).toBe("Jan 5");
  });

  it("names the month", () => {
    expect(formatDay("2026-07-27")).toBe("Jul 27");
  });
});
