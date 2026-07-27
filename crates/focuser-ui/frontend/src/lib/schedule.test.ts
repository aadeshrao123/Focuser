import { describe, expect, it } from "vitest";
import { type CellKey, cellKey, cellsToSlots, slotsToCells } from "./schedule";

const cells = (...keys: CellKey[]) => new Set(keys);

describe("cellsToSlots", () => {
  it("merges contiguous hours into one slot", () => {
    const slots = cellsToSlots(cells("Mon-9", "Mon-10", "Mon-11"));

    expect(slots).toEqual([{ day: "Mon", start: "09:00:00", end: "12:00:00" }]);
  });

  it("splits non-contiguous hours", () => {
    const slots = cellsToSlots(cells("Mon-9", "Mon-10", "Mon-14"));

    expect(slots).toEqual([
      { day: "Mon", start: "09:00:00", end: "11:00:00" },
      { day: "Mon", start: "14:00:00", end: "15:00:00" },
    ]);
  });

  it("ends the last hour of the day at midnight", () => {
    expect(cellsToSlots(cells("Sun-23"))).toEqual([
      { day: "Sun", start: "23:00:00", end: "00:00:00" },
    ]);
  });

  it("keeps days separate", () => {
    const slots = cellsToSlots(cells("Mon-9", "Tue-9"));

    expect(slots).toHaveLength(2);
    expect(slots.map((s) => s.day)).toEqual(["Mon", "Tue"]);
  });
});

describe("slotsToCells", () => {
  it("expands a slot into one cell per hour", () => {
    const set = slotsToCells([{ day: "Mon", start: "09:00:00", end: "12:00:00" }]);

    expect([...set].sort()).toEqual(["Mon-10", "Mon-11", "Mon-9"]);
  });

  it("reads a midnight end as the end of the day", () => {
    const set = slotsToCells([{ day: "Sun", start: "22:00:00", end: "00:00:00" }]);

    expect([...set].sort()).toEqual(["Sun-22", "Sun-23"]);
  });

  it("ignores days it does not recognise", () => {
    expect(slotsToCells([{ day: "Funday", start: "09:00:00", end: "10:00:00" }]).size).toBe(0);
  });
});

describe("round trip", () => {
  it("survives a full week", () => {
    const original = new Set<CellKey>();
    for (const day of ["Mon", "Wed", "Sun"] as const) {
      for (const hour of [0, 1, 9, 10, 11, 23]) original.add(cellKey(day, hour));
    }

    expect(slotsToCells(cellsToSlots(original))).toEqual(original);
  });

  it("collapses a full week into seven slots", () => {
    const all = new Set<CellKey>();
    for (const day of ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const) {
      for (let h = 0; h < 24; h++) all.add(cellKey(day, h));
    }

    // 168 selected hours should not become 168 rows in the database.
    expect(cellsToSlots(all)).toHaveLength(7);
  });
});
