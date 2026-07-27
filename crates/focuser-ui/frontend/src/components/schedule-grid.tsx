import { type Dispatch, type SetStateAction, useCallback, useEffect, useRef } from "react";
import { type CellKey, cellKey, DAYS, type Day, formatHour, HOURS } from "@/lib/schedule";
import { cn } from "@/lib/utils";

/**
 * 7×24 hour grid with click-and-drag painting.
 *
 * Dragging paints one mode for the whole gesture, decided by the first cell:
 * starting on a selected hour erases, starting on an empty one fills. Without
 * that, a drag across mixed cells just inverts them and feels random.
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

  function startPaint(key: CellKey) {
    if (disabled) return;
    const mode = selected.has(key) ? "remove" : "add";
    paintMode.current = mode;
    apply(key, mode);
  }

  function paintOver(key: CellKey) {
    if (paintMode.current) apply(key, paintMode.current);
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[42rem] select-none border-separate border-spacing-px">
        <caption className="sr-only">Weekly schedule, one column per hour</caption>
        <thead>
          <tr>
            <th />
            {HOURS.map((h) => (
              <th
                key={h}
                scope="col"
                className="pb-1 text-center font-normal text-[10px] text-faint-foreground"
              >
                {h % 3 === 0 ? formatHour(h) : <span className="sr-only">{formatHour(h)}</span>}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {DAYS.map((day) => (
            <tr key={day}>
              <th
                scope="row"
                className="w-12 pr-2 text-left font-normal text-muted-foreground text-xs"
              >
                {day}
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
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Cell({
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
    <td className="p-0">
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
          "block h-7 w-full rounded-[4px] transition-colors duration-100",
          "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring",
          on
            ? "bg-primary shadow-(--shadow-depth-sm) hover:bg-primary-hover"
            : "bg-elevated/70 hover:bg-hover",
          disabled && "cursor-not-allowed opacity-50",
        )}
      />
    </td>
  );
}
