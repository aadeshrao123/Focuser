import { type VariantProps, cva } from "class-variance-authority";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Card } from "./card";

const accent = cva("", {
  variants: {
    tone: {
      default: "text-muted-foreground",
      primary: "text-primary",
      success: "text-success",
      warning: "text-warning",
      destructive: "text-destructive",
    },
  },
  defaultVariants: { tone: "default" },
});

const wash = cva("pointer-events-none absolute inset-x-0 -top-16 h-32 opacity-0 blur-2xl", {
  variants: {
    tone: {
      default: "",
      primary: "bg-primary/25 opacity-100",
      success: "bg-success/20 opacity-100",
      warning: "bg-warning/20 opacity-100",
      destructive: "bg-destructive/20 opacity-100",
    },
  },
  defaultVariants: { tone: "default" },
});

export interface StatProps extends VariantProps<typeof accent> {
  label: string;
  value: ReactNode;
  hint?: string;
  icon?: ReactNode;
  /** Rendered under the value — a sparkline, a bar, a badge. */
  footer?: ReactNode;
  className?: string;
}

/**
 * One headline number.
 *
 * A toned stat gets a coloured glow bleeding down from above the card, which
 * is what separates the number you are meant to read first from the three
 * beside it. Lay several out with `StatGrid`.
 */
export function Stat({ label, value, hint, icon, footer, tone, className }: StatProps) {
  return (
    <Card
      elevation="raised"
      className={cn("relative min-w-0 overflow-hidden", className)}
      padding="md"
    >
      <div aria-hidden className={wash({ tone })} />

      <div className="relative">
        <div className="flex items-center gap-2 text-muted-foreground text-xs">
          <span className={cn("[&_svg]:size-3.5", accent({ tone }))}>{icon}</span>
          <span className="truncate">{label}</span>
        </div>

        <p className="mt-2.5 truncate font-semibold text-[1.65rem] text-foreground leading-none tabular-nums tracking-tight">
          {value}
        </p>

        {hint && <p className="mt-2 truncate text-faint-foreground text-xs">{hint}</p>}
        {footer && <div className="mt-3">{footer}</div>}
      </div>
    </Card>
  );
}

export function StatGrid({ children }: { children: ReactNode }) {
  return <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">{children}</div>;
}
