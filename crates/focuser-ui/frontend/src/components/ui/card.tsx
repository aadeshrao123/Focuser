import { type VariantProps, cva } from "class-variance-authority";
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/utils";

const cardVariants = cva("rounded-lg border border-border bg-surface", {
  variants: {
    padding: {
      none: "",
      sm: "p-3",
      md: "p-4",
      lg: "p-6",
    },
    interactive: {
      true: "transition-colors hover:border-border-strong hover:bg-elevated",
      false: "",
    },
    elevation: {
      flat: "",
      raised: "shadow-(--shadow-depth-sm)",
      floating: "shadow-(--shadow-depth-md)",
    },
  },
  defaultVariants: { padding: "md", interactive: false, elevation: "flat" },
});

export interface CardProps
  extends HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof cardVariants> {}

export function Card({ className, padding, interactive, elevation, ...props }: CardProps) {
  return (
    <div
      className={cn(cardVariants({ padding, interactive, elevation }), className)}
      {...props}
    />
  );
}

/** Page-level heading with optional description and right-aligned actions. */
export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-6 flex items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="font-semibold text-2xl text-foreground tracking-tight">{title}</h1>
        {description && <p className="mt-1 text-muted-foreground text-sm">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}

/** Shown when a list is genuinely empty — distinct from still loading. */
export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="rounded-lg border border-border border-dashed px-6 py-10 text-center">
      <p className="font-medium text-foreground text-sm">{title}</p>
      {description && <p className="mt-1 text-muted-foreground text-sm">{description}</p>}
      {action && <div className="mt-4 flex justify-center">{action}</div>}
    </div>
  );
}
