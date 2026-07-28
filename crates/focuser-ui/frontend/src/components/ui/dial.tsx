import { useCallback, useId, useRef } from "react";
import { cn } from "@/lib/utils";

const SIZE = 116;
const STROKE = 9;
const R = (SIZE - STROKE) / 2;
const C = 2 * Math.PI * R;
/** 270°, with the gap at the bottom, so the two ends are visibly different. */
const SWEEP = 0.75;
const START = 135;

const clamp = (n: number, min: number, max: number) => Math.min(max, Math.max(min, n));

/**
 * A dial for a duration — drag round it, or focus it and use the arrow keys.
 *
 * A number field states a value; this shows how big it is next to its
 * neighbours, which is the actual question when you are balancing focus
 * against breaks.
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
  const dragging = useRef(false);
  const labelId = useId();

  const fraction = (clamp(value, min, max) - min) / (max - min || 1);

  const fromPointer = useCallback(
    (clientX: number, clientY: number) => {
      const box = svg.current?.getBoundingClientRect();
      if (!box) return;

      const dx = clientX - (box.left + box.width / 2);
      const dy = clientY - (box.top + box.height / 2);
      // atan2 with y down gives degrees clockwise from 3 o'clock, which is
      // where the arc starts once START is subtracted.
      const degrees = (((Math.atan2(dy, dx) * 180) / Math.PI - START + 360) % 360) / (SWEEP * 360);

      // Past the end, snap to whichever end is nearer rather than wrapping.
      const t = degrees > 1 ? (degrees > 1 + (1 / SWEEP - 1) / 2 ? 0 : 1) : degrees;
      const next = Math.round((min + t * (max - min)) / step) * step;
      onChange(clamp(next, min, max));
    },
    [max, min, onChange, step],
  );

  const onKeyDown = (event: React.KeyboardEvent) => {
    const big = event.shiftKey ? 5 : 1;
    const moves: Record<string, number> = {
      ArrowUp: step * big,
      ArrowRight: step * big,
      ArrowDown: -step * big,
      ArrowLeft: -step * big,
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
    <div className="flex flex-col items-center gap-2">
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
          dragging.current = true;
          fromPointer(e.clientX, e.clientY);
          // Capture keeps the drag alive outside the circle, but must not be
          // what decides whether the press registered at all.
          try {
            e.currentTarget.setPointerCapture(e.pointerId);
          } catch {
            /* no capture available */
          }
        }}
        onPointerMove={(e) => dragging.current && fromPointer(e.clientX, e.clientY)}
        onPointerUp={() => {
          dragging.current = false;
        }}
        onPointerCancel={() => {
          dragging.current = false;
        }}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className={cn(
          "size-[7.25rem] cursor-pointer touch-none select-none",
          "rounded-full focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
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
            strokeDasharray={`${SWEEP * C} ${C}`}
          />
          <circle
            cx={SIZE / 2}
            cy={SIZE / 2}
            r={R}
            fill="none"
            stroke={color}
            strokeWidth={STROKE}
            strokeLinecap="round"
            strokeDasharray={`${fraction * SWEEP * C} ${C}`}
            className="transition-[stroke-dasharray] duration-150"
          />
        </g>

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

      <span id={labelId} className="text-muted-foreground text-xs">
        {label}
      </span>
    </div>
  );
}
