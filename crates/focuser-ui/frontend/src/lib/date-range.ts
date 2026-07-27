/**
 * Calendar-date helpers for range pickers.
 *
 * Everything works on `YYYY-MM-DD` strings in *local* time, which is what the
 * backend's `NaiveDate` means. `toISOString()` is deliberately avoided — it
 * converts to UTC and shifts the day for anyone west of Greenwich.
 */

export type RangeId = "today" | "7d" | "30d" | "90d";

export interface DateRange {
  from: string;
  to: string;
}

export const RANGES = [
  { value: "today", label: "Today" },
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
  { value: "90d", label: "Last 90 days" },
] as const satisfies readonly { value: RangeId; label: string }[];

const SPAN: Record<RangeId, number> = { today: 1, "7d": 7, "30d": 30, "90d": 90 };

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export function toIsoDate(date: Date): string {
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

export function parseIsoDate(iso: string): Date {
  const [year, month, day] = iso.split("-").map(Number);
  return new Date(year, month - 1, day);
}

/** Ranges are inclusive of both ends, so "last 7 days" spans today and six before it. */
export function rangeFor(id: RangeId, today: Date = new Date()): DateRange {
  const start = new Date(today);
  start.setDate(start.getDate() - (SPAN[id] - 1));
  return { from: toIsoDate(start), to: toIsoDate(today) };
}

/** Every day in the range, so charts show empty days instead of skipping them. */
export function daysBetween({ from, to }: DateRange): string[] {
  const days: string[] = [];
  const cursor = parseIsoDate(from);
  const end = parseIsoDate(to);
  while (cursor <= end) {
    days.push(toIsoDate(cursor));
    cursor.setDate(cursor.getDate() + 1);
  }
  return days;
}

/** "2026-07-27" → "Jul 27" */
export function formatDay(iso: string): string {
  const [, month, day] = iso.split("-");
  return `${MONTHS[Number(month) - 1]} ${Number(day)}`;
}
