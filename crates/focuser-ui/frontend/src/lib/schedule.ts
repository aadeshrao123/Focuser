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
    // An end of 00:00 means midnight at the end of the day, not the start.
    const end = rawEnd === 0 && start !== 0 ? 24 : rawEnd;

    for (let h = start; h < end; h++) cells.add(cellKey(day, h));
  }

  return cells;
}

/** Merge each day's selected hours into as few contiguous slots as possible. */
export function cellsToSlots(cells: Set<CellKey>): TimeSlot[] {
  const slots: TimeSlot[] = [];

  for (const day of DAYS) {
    const hours = HOURS.filter((h) => cells.has(cellKey(day, h)));
    if (hours.length === 0) continue;

    let runStart = hours[0];
    let prev = hours[0];

    for (const hour of hours.slice(1)) {
      if (hour !== prev + 1) {
        slots.push({ day, start: toTime(runStart), end: endTime(prev) });
        runStart = hour;
      }
      prev = hour;
    }

    slots.push({ day, start: toTime(runStart), end: endTime(prev) });
  }

  return slots;
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
