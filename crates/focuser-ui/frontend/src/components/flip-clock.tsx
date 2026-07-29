import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

type Size = "sm" | "lg";

const SIZES: Record<Size, React.CSSProperties> = {
  sm: { "--flip-w": "2.1rem", "--flip-h": "3rem", "--flip-size": "1.75rem", "--flip-r": "0.4rem" },
  lg: { "--flip-w": "3.1rem", "--flip-h": "4.4rem", "--flip-size": "2.6rem", "--flip-r": "0.6rem" },
} as Record<Size, React.CSSProperties>;

/**
 * A countdown that falls one digit at a time, like a split-flap board.
 *
 * Only the digits that actually changed animate, so a normal second flips one
 * card rather than the whole clock.
 */
export function FlipClock({
  seconds,
  size = "lg",
  paused,
  className,
}: {
  seconds: number;
  size?: Size;
  paused?: boolean;
  className?: string;
}) {
  const safe = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const secs = safe % 60;

  const groups = [
    ...(hours > 0 ? [{ unit: "h", value: hours }] : []),
    { unit: "m", value: minutes },
    { unit: "s", value: secs },
  ];
  const label = groups.map((g) => String(g.value).padStart(2, "0")).join(":");

  return (
    <div
      className={cn("flex items-center gap-1.5", paused && "opacity-60", className)}
      style={SIZES[size]}
      role="timer"
      aria-live="off"
      aria-label={m.timer_remaining({ time: label })}
    >
      {groups.map((group, i) => (
        <div key={group.unit} className="flex items-center gap-1.5">
          {i > 0 && <Separator size={size} />}
          <Digit value={Math.floor(group.value / 10) % 10} />
          <Digit value={group.value % 10} />
        </div>
      ))}
    </div>
  );
}

function Separator({ size }: { size: Size }) {
  return (
    <span
      aria-hidden
      className={cn(
        "flex flex-col justify-center gap-1.5 px-0.5 text-faint-foreground",
        size === "lg" ? "gap-2" : "gap-1.5",
      )}
    >
      <span className="block size-1 rounded-full bg-current" />
      <span className="block size-1 rounded-full bg-current" />
    </span>
  );
}

function Digit({ value }: { value: number }) {
  const [state, setState] = useState({ current: value, previous: value, turn: 0 });

  useEffect(() => {
    setState((s) =>
      s.current === value ? s : { current: value, previous: s.current, turn: s.turn + 1 },
    );
  }, [value]);

  const flipping = state.current !== state.previous;

  return (
    <div className="flip-digit shadow-(--shadow-depth-sm)">
      {/* Static: the top already shows the new digit, the bottom still shows
          the old one. The moving halves cover each until the hand-over. */}
      <div className="flip-half flip-half-top">
        <span>{state.current}</span>
      </div>
      <div className="flip-half flip-half-bottom">
        <span>{state.previous}</span>
      </div>

      {flipping && (
        <>
          <div key={`t${state.turn}`} className="flip-half flip-half-top flip-anim-top">
            <span>{state.previous}</span>
          </div>
          <div
            key={`b${state.turn}`}
            className="flip-half flip-half-bottom flip-anim-bottom"
            onAnimationEnd={() => setState((s) => ({ ...s, previous: s.current }))}
          >
            <span>{state.current}</span>
          </div>
        </>
      )}
    </div>
  );
}
