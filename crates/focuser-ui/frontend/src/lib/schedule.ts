import type { TimeSlot } from "@/bindings";

export const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
export type Day = (typeof DAYS)[number];

export const HOURS = Array.from({ length: 24 }, (_, h) => h);

/** A selected hour, keyed as `Mon-9`. */
export type CellKey = `${Day}-${number}`;

export const cellKey = (day: Day, hour: number): CellKey => `${day}-${hour}`;

const pad = (n: number) => String(n).padStart(2, "0");
const toTime = (hour: number) => `${pad(hour)}:00:00`;

/** Hour 23 ends at midnight; the backend treats that as a wrap. */
const endTime = (hour: number) => toTime((hour + 1) % 24);

export function slotsToCells(slots: TimeSlot[]): Set<CellKey> {
  const cells = new Set<CellKey>();

  for (const slot of slots) {
    const day = slot.day as Day;
    if (!DAYS.includes(day)) continue;

    const start = Number(slot.start.slice(0, 2));
    const rawEnd = Number(slot.end.slice(0, 2));
    // An end of 00:00 always means midnight at the end of the day. This used to
    // exclude slots starting at 00:00, which made a full day — saved as
    // 00:00–00:00 — read back as nothing at all. A zero-length slot is never
    // written, so there is no other reading of it.
    const end = rawEnd === 0 ? 24 : rawEnd;

    for (let h = start; h < end; h++) cells.add(cellKey(day, h));
  }

  return cells;
}

/** A day's selected hours, collapsed to `[start, endExclusive]` pairs. */
export function runsForDay(cells: Set<CellKey>, day: Day): [number, number][] {
  const hours = HOURS.filter((h) => cells.has(cellKey(day, h)));
  if (hours.length === 0) return [];

  const runs: [number, number][] = [];
  let start = hours[0];
  let prev = hours[0];

  for (const hour of hours.slice(1)) {
    if (hour !== prev + 1) {
      runs.push([start, prev + 1]);
      start = hour;
    }
    prev = hour;
  }

  runs.push([start, prev + 1]);
  return runs;
}

/** Merge each day's selected hours into as few contiguous slots as possible. */
export function cellsToSlots(cells: Set<CellKey>): TimeSlot[] {
  return DAYS.flatMap((day) =>
    runsForDay(cells, day).map(([start, end]) => ({
      day,
      start: toTime(start),
      end: endTime(end - 1),
    })),
  );
}

export function hoursOn(cells: Set<CellKey>, day: Day): number {
  return HOURS.reduce((n, h) => n + (cells.has(cellKey(day, h)) ? 1 : 0), 0);
}

/** "9am–5pm", "9am–12pm, 2–5pm", or "Off". Used in the week summary. */
export function describeDay(cells: Set<CellKey>, day: Day): string {
  const runs = runsForDay(cells, day);
  if (runs.length === 0) return "Off";
  if (runs.length === 1 && runs[0][0] === 0 && runs[0][1] === 24) return "All day";
  return runs.map(([start, end]) => `${formatHour(start)}–${formatHour(end % 24)}`).join(", ");
}

/** Fill a whole row, or clear it when it is already full. */
export function toggleDay(cells: Set<CellKey>, day: Day): Set<CellKey> {
  const next = new Set(cells);
  const full = hoursOn(cells, day) === HOURS.length;
  for (const hour of HOURS) {
    if (full) next.delete(cellKey(day, hour));
    else next.add(cellKey(day, hour));
  }
  return next;
}

/** Fill one hour across every day, or clear it when every day already has it. */
export function toggleHour(cells: Set<CellKey>, hour: number): Set<CellKey> {
  const next = new Set(cells);
  const full = DAYS.every((day) => cells.has(cellKey(day, hour)));
  for (const day of DAYS) {
    if (full) next.delete(cellKey(day, hour));
    else next.add(cellKey(day, hour));
  }
  return next;
}

const range = (from: number, to: number) => HOURS.filter((h) => h >= from && h < to);

export const PRESETS: { id: string; label: string; build: () => Set<CellKey> }[] = [
  {
    id: "work",
    label: "Work hours",
    build: () => fill(["Mon", "Tue", "Wed", "Thu", "Fri"], range(9, 17)),
  },
  {
    id: "evenings",
    label: "Evenings",
    build: () => fill([...DAYS], range(18, 24)),
  },
  {
    id: "weekends",
    label: "Weekends",
    build: () => fill(["Sat", "Sun"], HOURS),
  },
  {
    id: "always",
    label: "Every hour",
    build: () => fill([...DAYS], HOURS),
  },
];

function fill(days: Day[], hours: number[]): Set<CellKey> {
  const cells = new Set<CellKey>();
  for (const day of days) for (const hour of hours) cells.add(cellKey(day, hour));
  return cells;
}

export function formatHour(hour: number) {
  if (hour === 0) return "12am";
  if (hour === 12) return "12pm";
  return hour < 12 ? `${hour}am` : `${hour - 12}pm`;
}
