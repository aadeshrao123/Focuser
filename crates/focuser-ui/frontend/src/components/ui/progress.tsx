import { type VariantProps, cva } from "class-variance-authority";
import { cn } from "@/lib/utils";

const bar = cva("h-full rounded-full transition-[width] duration-1000 ease-linear", {
  variants: {
    tone: {
      default: "bg-primary",
      success: "bg-success",
      warning: "bg-warning",
      destructive: "bg-destructive",
    },
  },
  defaultVariants: { tone: "default" },
});

export interface ProgressProps extends VariantProps<typeof bar> {
  /** 0 to 1. Anything outside that is clamped. */
  value: number;
  label: string;
  className?: string;
}

export function Progress({ value, label, tone, className }: ProgressProps) {
  const pct = Math.round(Math.min(1, Math.max(0, value)) * 100);

  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuenow={pct}
      className={cn("h-1.5 overflow-hidden rounded-full bg-elevated", className)}
    >
      <div className={bar({ tone })} style={{ width: `${pct}%` }} />
    </div>
  );
}
