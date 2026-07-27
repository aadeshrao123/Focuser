import { type VariantProps, cva } from "class-variance-authority";
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/utils";

const cardVariants = cva("rounded-xl border border-border bg-surface", {
  variants: {
    padding: {
      none: "",
      sm: "p-3",
      md: "p-4",
      lg: "p-6",
    },
    interactive: {
      // Lifts slightly and warms its border. Cheap to composite, and the
      // border change alone would be too subtle to read as clickable.
      true: [
        "cursor-pointer transition-[transform,box-shadow,border-color,background-color] duration-200",
        "hover:-translate-y-0.5 hover:border-border-strong hover:bg-elevated",
        "hover:shadow-(--shadow-depth-md)",
      ],
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
    <header className="mb-7 flex items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="font-semibold text-[1.7rem] text-foreground leading-tight tracking-tight">
          {title}
        </h1>
        {description && (
          <p className="mt-1.5 max-w-2xl text-muted-foreground text-sm">{description}</p>
        )}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}

/** A titled block within a page. */
export function Section({
  title,
  description,
  actions,
  children,
  className,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("mt-7", className)}>
      <div className="mb-3 flex items-end justify-between gap-4">
        <div className="min-w-0">
          <h2 className="font-medium text-foreground text-sm">{title}</h2>
          {description && (
            <p className="mt-0.5 text-muted-foreground text-xs">{description}</p>
          )}
        </div>
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

/**
 * Shown when a list is genuinely empty — distinct from still loading.
 *
 * The icon is what stops an empty page reading as a broken one.
 */
export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="rounded-xl border border-border border-dashed bg-surface/40 px-6 py-12 text-center">
      {icon && (
        <div className="mx-auto mb-3 flex size-11 items-center justify-center rounded-full bg-elevated text-muted-foreground [&_svg]:size-5">
          {icon}
        </div>
      )}
      <p className="font-medium text-foreground text-sm">{title}</p>
      {description && (
        <p className="mx-auto mt-1.5 max-w-sm text-muted-foreground text-sm">{description}</p>
      )}
      {action && <div className="mt-5 flex justify-center">{action}</div>}
    </div>
  );
}
