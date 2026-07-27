import { describe, expect, it } from "vitest";
import {
  type CellKey,
  cellKey,
  cellsToSlots,
  DAYS,
  describeDay,
  HOURS,
  hoursOn,
  slotsToCells,
  toggleDay,
  toggleHour,
} from "./schedule";

const cells = (...keys: CellKey[]) => new Set(keys);

const wholeWeek = () => {
  const all = new Set<CellKey>();
  for (const day of DAYS) for (const hour of HOURS) all.add(cellKey(day, hour));
  return all;
};

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

  // A whole day is stored as 00:00–00:00. Reading the end as the *start* of
  // the day made it expand to nothing, so a day filled in one click came back
  // empty after a reload.
  it("expands a midnight-to-midnight slot into the whole day", () => {
    const set = slotsToCells([{ day: "Mon", start: "00:00:00", end: "00:00:00" }]);

    expect(set.size).toBe(24);
    expect(describeDay(set, "Mon")).toBe("All day");
  });

  it("ignores days it does not recognise", () => {
    expect(slotsToCells([{ day: "Funday", start: "09:00:00", end: "10:00:00" }]).size).toBe(0);
  });
});

describe("describeDay", () => {
  it("reads one run as a range", () => {
    expect(describeDay(cells("Mon-9", "Mon-10", "Mon-11"), "Mon")).toBe("9am–12pm");
  });

  it("lists each run when the day is split", () => {
    expect(describeDay(cells("Mon-9", "Mon-10", "Mon-14", "Mon-15"), "Mon")).toBe(
      "9am–11am, 2pm–4pm",
    );
  });

  it("names a full day rather than spelling out midnight to midnight", () => {
    expect(describeDay(wholeWeek(), "Wed")).toBe("All day");
  });

  it("ends the last hour of the day at midnight, not 12pm", () => {
    expect(describeDay(cells("Sun-22", "Sun-23"), "Sun")).toBe("10pm–12am");
  });

  it("says so when nothing is selected", () => {
    expect(describeDay(cells(), "Fri")).toBe("Off");
  });
});

describe("toggleDay", () => {
  it("fills a partly-selected row rather than clearing it", () => {
    expect(hoursOn(toggleDay(cells("Mon-9"), "Mon"), "Mon")).toBe(24);
  });

  it("clears a row that is already full", () => {
    expect(hoursOn(toggleDay(wholeWeek(), "Mon"), "Mon")).toBe(0);
  });

  it("leaves the other days alone", () => {
    const next = toggleDay(cells("Tue-3"), "Mon");

    expect(next.has("Tue-3")).toBe(true);
    expect(hoursOn(next, "Tue")).toBe(1);
  });
});

describe("toggleHour", () => {
  it("fills the column across every day", () => {
    const next = toggleHour(cells("Mon-9"), 9);

    expect(DAYS.every((day) => next.has(cellKey(day, 9)))).toBe(true);
  });

  it("clears a column that every day already has", () => {
    const next = toggleHour(wholeWeek(), 9);

    expect(DAYS.some((day) => next.has(cellKey(day, 9)))).toBe(false);
    // Only that hour goes; the rest of the week is untouched.
    expect(next.size).toBe(24 * 7 - 7);
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

  it("survives a full week that is entirely selected", () => {
    const all = wholeWeek();

    expect(slotsToCells(cellsToSlots(all))).toEqual(all);
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
