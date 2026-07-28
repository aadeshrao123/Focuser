import { useCallback, useId, useRef, useState } from "react";
import { cn } from "@/lib/utils";

const SIZE = 116;
const STROKE = 9;
const R = (SIZE - STROKE) / 2;
const C = 2 * Math.PI * R;
/** 270°, with the gap at the bottom, so the two ends are visibly different. */
const SWEEP_DEG = 270;
const GAP_DEG = 360 - SWEEP_DEG;
const START = 135;
/** Near the middle the angle swings wildly for a tiny movement. */
const DEAD_ZONE = 0.3;

const clamp = (n: number, min: number, max: number) => Math.min(max, Math.max(min, n));
const radians = (deg: number) => (deg * Math.PI) / 180;

/**
 * A dial for a duration — drag round it, or focus it and use the arrow keys.
 *
 * A number field states a value; this shows how big it is next to its
 * neighbours, which is the actual question when balancing focus against rest.
 */
export function Dial({
  value,
  onChange,
  min,
  max,
  step = 1,
  label,
  suffix,
  color = "var(--color-primary)",
}: {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  label: string;
  suffix?: string;
  color?: string;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const lastFraction = useRef<number | null>(null);
  const [dragging, setDragging] = useState(false);
  const labelId = useId();

  const fraction = (clamp(value, min, max) - min) / (max - min || 1);
  const knob = START + fraction * SWEEP_DEG;

  const fromPointer = useCallback(
    (clientX: number, clientY: number) => {
      const box = svg.current?.getBoundingClientRect();
      if (!box) return;

      const dx = clientX - (box.left + box.width / 2);
      const dy = clientY - (box.top + box.height / 2);
      if (Math.hypot(dx, dy) < (box.width / 2) * DEAD_ZONE) return;

      // atan2 with y down gives degrees clockwise from 3 o'clock; subtracting
      // START re-bases them onto the sweep, which begins at bottom-left.
      const angle = ((Math.atan2(dy, dx) * 180) / Math.PI - START + 360) % 360;

      let next: number;
      if (angle <= SWEEP_DEG) next = angle / SWEEP_DEG;
      // In the gap under the dial there is no value, so hold at whichever end
      // is nearer rather than letting the pointer wrap round.
      else next = angle - SWEEP_DEG < GAP_DEG / 2 ? 1 : 0;

      // A drag that appears to leap across the dial has really crossed the
      // gap. Ignoring it is what stops the value flipping between its ends.
      const previous = lastFraction.current;
      if (previous !== null && Math.abs(next - previous) > 0.5) return;

      lastFraction.current = next;
      const stepped = Math.round((min + next * (max - min)) / step) * step;
      onChange(clamp(stepped, min, max));
    },
    [max, min, onChange, step],
  );

  const stop = () => {
    setDragging(false);
    lastFraction.current = null;
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const jump = event.shiftKey ? 5 : 1;
    const moves: Record<string, number> = {
      ArrowUp: step * jump,
      ArrowRight: step * jump,
      ArrowDown: -step * jump,
      ArrowLeft: -step * jump,
      PageUp: step * 10,
      PageDown: -step * 10,
    };
    const delta = moves[event.key];

    if (delta !== undefined) {
      event.preventDefault();
      onChange(clamp(value + delta, min, max));
    } else if (event.key === "Home") {
      event.preventDefault();
      onChange(min);
    } else if (event.key === "End") {
      event.preventDefault();
      onChange(max);
    }
  };

  return (
    <div className="group flex flex-col items-center gap-2">
      {/** biome-ignore lint/a11y/useSemanticElements: a range input cannot be a dial */}
      <svg
        ref={svg}
        role="slider"
        tabIndex={0}
        aria-labelledby={labelId}
        aria-valuenow={value}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuetext={suffix ? `${value} ${suffix}` : String(value)}
        onKeyDown={onKeyDown}
        onPointerDown={(e) => {
          e.preventDefault();
          setDragging(true);
          // Seeded from where the value already is, so the first move is
          // measured against the knob rather than against nothing.
          lastFraction.current = fraction;
          fromPointer(e.clientX, e.clientY);
          // Capture keeps the drag alive outside the circle, but must not be
          // what decides whether the press registered at all.
          try {
            e.currentTarget.setPointerCapture(e.pointerId);
          } catch {
            /* no capture available */
          }
        }}
        onPointerMove={(e) => {
          if (dragging) fromPointer(e.clientX, e.clientY);
        }}
        onPointerUp={stop}
        onPointerCancel={stop}
        onLostPointerCapture={stop}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className={cn(
          "size-[7.25rem] touch-none select-none rounded-full",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
          dragging ? "cursor-grabbing" : "cursor-grab",
        )}
      >
        <title>{label}</title>
        <g transform={`rotate(${START} ${SIZE / 2} ${SIZE / 2})`}>
          <circle
            cx={SIZE / 2}
            cy={SIZE / 2}
            r={R}
            fill="none"
            stroke="var(--color-elevated)"
            strokeWidth={STROKE}
            strokeLinecap="round"
            strokeDasharray={`${(SWEEP_DEG / 360) * C} ${C}`}
            className="transition-colors group-hover:stroke-[var(--color-hover)]"
          />
          <circle
            cx={SIZE / 2}
            cy={SIZE / 2}
            r={R}
            fill="none"
            stroke={color}
            strokeWidth={STROKE}
            strokeLinecap="round"
            strokeDasharray={`${fraction * (SWEEP_DEG / 360) * C} ${C}`}
            // No transition while dragging: easing every step makes the arc
            // lag behind the pointer.
            className={dragging ? undefined : "transition-[stroke-dasharray] duration-150"}
          />
        </g>

        {/* The handle. Mostly what tells you this can be dragged at all. */}
        <circle
          cx={SIZE / 2 + R * Math.cos(radians(knob))}
          cy={SIZE / 2 + R * Math.sin(radians(knob))}
          r={dragging ? 7 : 5.5}
          fill="var(--color-foreground)"
          stroke={color}
          strokeWidth={2.5}
          className="opacity-80 transition-[r,opacity] group-hover:opacity-100"
        />

        <text
          x="50%"
          y="49%"
          textAnchor="middle"
          dominantBaseline="middle"
          className="fill-foreground font-semibold text-[1.65rem] tabular-nums"
        >
          {value}
        </text>
        {suffix && (
          <text
            x="50%"
            y="68%"
            textAnchor="middle"
            dominantBaseline="middle"
            className="fill-faint-foreground text-[0.62rem] uppercase tracking-widest"
          >
            {suffix}
          </text>
        )}
      </svg>

      <span
        id={labelId}
        className="text-muted-foreground text-xs transition-colors group-hover:text-foreground"
      >
        {label}
      </span>
    </div>
  );
}
