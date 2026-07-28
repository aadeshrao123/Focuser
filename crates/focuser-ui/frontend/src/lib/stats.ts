import type { UsageStat } from "@/bindings";
import { type DateRange, daysBetween } from "./date-range";

export interface DayTotal {
  date: string;
  attempts: number;
  seconds: number;
}

export interface TargetTotal {
  target: string;
  attempts: number;
  seconds: number;
}

export interface Totals {
  attempts: number;
  seconds: number;
  targets: number;
  busiestDay: DayTotal | null;
}

/** One point per calendar day, zero-filled across the whole range. */
export function totalsByDay(stats: UsageStat[], range: DateRange): DayTotal[] {
  const days = new Map<string, DayTotal>();
  for (const date of daysBetween(range)) days.set(date, { date, attempts: 0, seconds: 0 });

  for (const stat of stats) {
    const day = days.get(stat.date);
    // Backend rows outside the requested range would distort the chart's axis.
    if (!day) continue;
    day.attempts += stat.blocked_attempts;
    day.seconds += stat.duration_seconds;
  }

  return [...days.values()];
}

/** Per site or app across the whole range, busiest first. */
export function totalsByTarget(stats: UsageStat[]): TargetTotal[] {
  const targets = new Map<string, TargetTotal>();

  for (const stat of stats) {
    const entry = targets.get(stat.domain_or_app) ?? {
      target: stat.domain_or_app,
      attempts: 0,
      seconds: 0,
    };
    entry.attempts += stat.blocked_attempts;
    entry.seconds += stat.duration_seconds;
    targets.set(entry.target, entry);
  }

  return [...targets.values()].sort(
    (a, b) => b.attempts - a.attempts || b.seconds - a.seconds || a.target.localeCompare(b.target),
  );
}

export interface TargetSeries extends TargetTotal {
  /** Zero-filled across the range, so every series shares one x axis. */
  days: DayTotal[];
}

/** Per site or app, but keeping the daily shape rather than collapsing it. */
export function seriesByTarget(stats: UsageStat[], range: DateRange): TargetSeries[] {
  const dates = daysBetween(range);
  const position = new Map(dates.map((date, i) => [date, i]));
  const byTarget = new Map<string, TargetSeries>();

  for (const stat of stats) {
    const at = position.get(stat.date);
    if (at === undefined) continue;

    let entry = byTarget.get(stat.domain_or_app);
    if (!entry) {
      entry = {
        target: stat.domain_or_app,
        attempts: 0,
        seconds: 0,
        days: dates.map((date) => ({ date, attempts: 0, seconds: 0 })),
      };
      byTarget.set(entry.target, entry);
    }

    const day = entry.days[at];
    if (!day) continue;
    day.attempts += stat.blocked_attempts;
    day.seconds += stat.duration_seconds;
    entry.attempts += stat.blocked_attempts;
    entry.seconds += stat.duration_seconds;
  }

  return [...byTarget.values()].sort(
    (a, b) => b.attempts - a.attempts || b.seconds - a.seconds || a.target.localeCompare(b.target),
  );
}

export function summarise(days: DayTotal[], targets: TargetTotal[]): Totals {
  const busiest = days.reduce<DayTotal | null>(
    (best, day) => (day.attempts > (best?.attempts ?? 0) ? day : best),
    null,
  );

  return {
    attempts: days.reduce((n, d) => n + d.attempts, 0),
    seconds: days.reduce((n, d) => n + d.seconds, 0),
    targets: targets.length,
    busiestDay: busiest,
  };
}
