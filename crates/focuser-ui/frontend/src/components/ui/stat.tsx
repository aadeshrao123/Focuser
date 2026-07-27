import { type VariantProps, cva } from "class-variance-authority";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Card } from "./card";

const statValue = cva("font-semibold text-2xl text-foreground tabular-nums tracking-tight", {
  variants: {
    tone: {
      default: "",
      primary: "text-primary",
      success: "text-success",
      warning: "text-warning",
      destructive: "text-destructive",
    },
  },
  defaultVariants: { tone: "default" },
});

export interface StatProps extends VariantProps<typeof statValue> {
  label: string;
  value: ReactNode;
  hint?: string;
  icon?: ReactNode;
  className?: string;
}

/** One headline number. Lay several out with `StatGrid`. */
export function Stat({ label, value, hint, icon, tone, className }: StatProps) {
  return (
    <Card className={cn("min-w-0", className)}>
      <div className="flex items-center gap-2 text-muted-foreground text-xs">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <p className={cn("mt-2 truncate", statValue({ tone }))}>{value}</p>
      {hint && <p className="mt-1 truncate text-faint-foreground text-xs">{hint}</p>}
    </Card>
  );
}

export function StatGrid({ children }: { children: ReactNode }) {
  return <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">{children}</div>;
}
