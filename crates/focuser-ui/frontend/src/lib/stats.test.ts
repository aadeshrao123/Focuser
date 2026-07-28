import { describe, expect, it } from "vitest";
import type { UsageStat } from "@/bindings";
import { seriesByTarget, summarise, totalsByDay, totalsByTarget } from "./stats";

const stat = (date: string, target: string, attempts: number, seconds = 0): UsageStat => ({
  date,
  domain_or_app: target,
  blocked_attempts: attempts,
  duration_seconds: seconds,
});

const range = { from: "2026-07-25", to: "2026-07-27" };

describe("totalsByDay", () => {
  it("zero-fills days with nothing recorded", () => {
    const days = totalsByDay([stat("2026-07-26", "reddit.com", 3)], range);

    expect(days).toHaveLength(3);
    expect(days.map((d) => d.attempts)).toEqual([0, 3, 0]);
  });

  it("sums several targets on the same day", () => {
    const days = totalsByDay(
      [stat("2026-07-25", "reddit.com", 2, 30), stat("2026-07-25", "x.com", 5, 90)],
      range,
    );

    expect(days[0]).toEqual({ date: "2026-07-25", attempts: 7, seconds: 120 });
  });

  it("ignores rows outside the range", () => {
    const days = totalsByDay([stat("2026-07-01", "reddit.com", 9)], range);

    expect(days.every((d) => d.attempts === 0)).toBe(true);
  });
});

describe("totalsByTarget", () => {
  it("merges a target across days and sorts busiest first", () => {
    const totals = totalsByTarget([
      stat("2026-07-25", "reddit.com", 2),
      stat("2026-07-26", "reddit.com", 3),
      stat("2026-07-26", "x.com", 10),
    ]);

    expect(totals.map((t) => t.target)).toEqual(["x.com", "reddit.com"]);
    expect(totals[1].attempts).toBe(5);
  });

  it("breaks ties on time, then alphabetically", () => {
    const totals = totalsByTarget([
      stat("2026-07-25", "b.com", 1, 10),
      stat("2026-07-25", "a.com", 1, 10),
      stat("2026-07-25", "c.com", 1, 99),
    ]);

    expect(totals.map((t) => t.target)).toEqual(["c.com", "a.com", "b.com"]);
  });
});

describe("summarise", () => {
  it("reports the busiest day", () => {
    const days = totalsByDay(
      [stat("2026-07-25", "a.com", 1), stat("2026-07-27", "a.com", 8)],
      range,
    );

    expect(summarise(days, totalsByTarget([])).busiestDay?.date).toBe("2026-07-27");
  });

  it("has no busiest day when nothing was blocked", () => {
    expect(summarise(totalsByDay([], range), []).busiestDay).toBeNull();
  });
});

describe("seriesByTarget", () => {
  it("keeps one zero-filled row per day for every target", () => {
    const series = seriesByTarget(
      [stat("2026-07-26", "reddit.com", 3), stat("2026-07-27", "reddit.com", 1)],
      range,
    );

    expect(series).toHaveLength(1);
    expect(series[0]?.days.map((d) => d.attempts)).toEqual([0, 3, 1]);
    expect(series[0]?.attempts).toBe(4);
  });

  it("gives every target the same x axis, so panels line up", () => {
    const series = seriesByTarget(
      [stat("2026-07-25", "a.com", 1), stat("2026-07-27", "b.com", 1)],
      range,
    );

    expect(series.map((s) => s.days.map((d) => d.date))).toEqual([
      ["2026-07-25", "2026-07-26", "2026-07-27"],
      ["2026-07-25", "2026-07-26", "2026-07-27"],
    ]);
  });

  it("sorts busiest first", () => {
    const series = seriesByTarget(
      [stat("2026-07-26", "quiet.com", 1), stat("2026-07-26", "loud.com", 9)],
      range,
    );
    expect(series.map((s) => s.target)).toEqual(["loud.com", "quiet.com"]);
  });

  it("drops rows outside the range rather than distorting the axis", () => {
    const series = seriesByTarget([stat("2026-01-01", "old.com", 5)], range);
    expect(series).toEqual([]);
  });
});
