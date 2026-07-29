import { type Dispatch, memo, type SetStateAction, useCallback, useEffect, useRef } from "react";
import {
  type CellKey,
  cellKey,
  DAYS,
  type Day,
  formatHour,
  HOURS,
  hoursOn,
  toggleDay,
  toggleHour,
} from "@/lib/schedule";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

/**
 * 7×24 hour grid with click-and-drag painting.
 *
 * Dragging paints one mode for the whole gesture, decided by the first cell:
 * starting on a selected hour erases, starting on an empty one fills. Without
 * that, a drag across mixed cells just inverts them and feels random.
 *
 * The day and hour headings are buttons too — filling a row or a column is the
 * common case, and dragging 24 cells to get there is busywork.
 */
export function ScheduleGrid({
  selected,
  onChange,
  disabled,
}: {
  selected: Set<CellKey>;
  onChange: Dispatch<SetStateAction<Set<CellKey>>>;
  disabled?: boolean;
}) {
  // A ref, not state: pointerenter can fire before React commits, and a stale
  // paint mode would drop cells mid-drag.
  const paintMode = useRef<"add" | "remove" | null>(null);

  // The pointer often leaves the grid before release, so listen on the window.
  useEffect(() => {
    const stop = () => {
      paintMode.current = null;
    };
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    return () => {
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
  }, []);

  // Functional update so a burst of pointerenter events accumulates instead of
  // each one rebuilding from the same pre-drag set.
  const apply = useCallback(
    (key: CellKey, mode: "add" | "remove") => {
      onChange((prev) => {
        const next = new Set(prev);
        if (mode === "add") next.add(key);
        else next.delete(key);
        return next;
      });
    },
    [onChange],
  );

  const startPaint = useCallback(
    (key: CellKey) => {
      if (disabled) return;
      // Read from the live set inside the updater so the mode matches what is
      // actually on screen, not a `selected` captured one render ago.
      onChange((prev) => {
        const mode = prev.has(key) ? "remove" : "add";
        paintMode.current = mode;
        const next = new Set(prev);
        if (mode === "add") next.add(key);
        else next.delete(key);
        return next;
      });
    },
    [disabled, onChange],
  );

  const paintOver = useCallback(
    (key: CellKey) => {
      if (paintMode.current) apply(key, paintMode.current);
    },
    [apply],
  );

  return (
    <div className="-mx-1 overflow-x-auto px-1 pb-1">
      {/* `table-fixed` is load-bearing. Auto layout sizes each column to its
          own heading, so the hours showing "12am" came out wide and the ones
          showing a dot came out narrow — 24 supposedly equal hours rendered at
          two different widths. */}
      <table className="w-full min-w-[60rem] table-fixed select-none border-separate border-spacing-[3px]">
        <caption className="sr-only">{m.grid_aria()}</caption>
        <thead>
          <tr>
            {/* Fixed layout takes every column width from the first row. */}
            <th className="w-20" />
            {HOURS.map((hour) => (
              <th key={hour} scope="col" className="pb-2">
                <button
                  type="button"
                  disabled={disabled}
                  onClick={() => onChange((prev) => toggleHour(prev, hour))}
                  title={m.schedule_toggle_hour({ hour: formatHour(hour) })}
                  className={cn(
                    "w-full rounded text-center font-normal text-[11px] text-faint-foreground",
                    "transition-colors hover:text-foreground disabled:pointer-events-none",
                  )}
                >
                  {hour % 3 === 0 ? (
                    formatHour(hour)
                  ) : (
                    <span className="sr-only">{formatHour(hour)}</span>
                  )}
                  {hour % 3 !== 0 && <span aria-hidden>·</span>}
                </button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {DAYS.map((day) => {
            const on = hoursOn(selected, day);
            return (
              <tr key={day}>
                <th scope="row" className="pr-2.5">
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => onChange((prev) => toggleDay(prev, day))}
                    title={m.schedule_toggle_day({ day })}
                    className={cn(
                      "flex w-full items-baseline justify-between gap-1 rounded px-1.5 py-1",
                      "font-normal text-sm transition-colors disabled:pointer-events-none",
                      on > 0 ? "text-foreground" : "text-muted-foreground",
                      "hover:bg-hover",
                    )}
                  >
                    <span>{day}</span>
                    <span
                      className={cn(
                        "text-[11px] tabular-nums",
                        on > 0 ? "text-primary" : "text-faint-foreground",
                      )}
                    >
                      {on}
                    </span>
                  </button>
                </th>
                {HOURS.map((hour) => (
                  <Cell
                    key={hour}
                    day={day}
                    hour={hour}
                    on={selected.has(cellKey(day, hour))}
                    disabled={disabled}
                    onStart={startPaint}
                    onEnter={paintOver}
                  />
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

// Memoised: a drag fires pointerenter continuously, and re-rendering all 168
// cells for a one-cell change is what made painting feel sticky.
const Cell = memo(function Cell({
  day,
  hour,
  on,
  disabled,
  onStart,
  onEnter,
}: {
  day: Day;
  hour: number;
  on: boolean;
  disabled?: boolean;
  onStart: (key: CellKey) => void;
  onEnter: (key: CellKey) => void;
}) {
  const key = cellKey(day, hour);

  return (
    <td className={cn("p-0", hour % 6 === 0 && hour !== 0 && "border-border/70 border-l")}>
      <button
        type="button"
        aria-pressed={on}
        aria-label={`${day} ${formatHour(hour)}`}
        disabled={disabled}
        onPointerDown={(e) => {
          // Keeps the gesture alive when the pointer leaves this cell.
          e.preventDefault();
          onStart(key);
        }}
        onPointerEnter={() => onEnter(key)}
        className={cn(
          "block h-11 w-full rounded-md transition-colors duration-100",
          "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring",
          on
            ? "bg-primary shadow-(--shadow-depth-sm) hover:bg-primary-hover"
            : "bg-elevated/60 hover:bg-hover",
          disabled && "cursor-not-allowed opacity-50",
        )}
      />
    </td>
  );
});
