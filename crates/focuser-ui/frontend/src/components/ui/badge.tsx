import { type VariantProps, cva } from "class-variance-authority";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

const badge = cva(
  "inline-flex items-center gap-1.5 whitespace-nowrap rounded-full font-medium [&_svg]:shrink-0",
  {
    variants: {
      tone: {
        neutral: "bg-elevated text-muted-foreground",
        primary: "bg-primary-dim text-primary",
        success: "bg-success/15 text-success",
        warning: "bg-warning/15 text-warning",
        destructive: "bg-destructive/15 text-destructive",
        info: "bg-info/15 text-info",
      },
      size: {
        sm: "px-1.5 py-0.5 text-[0.6875rem] [&_svg]:size-3",
        md: "px-2.5 py-1 text-xs [&_svg]:size-3.5",
      },
      outlined: {
        true: "ring-1 ring-current/25 ring-inset",
        false: "",
      },
    },
    defaultVariants: { tone: "neutral", size: "sm", outlined: false },
  },
);

export interface BadgeProps extends VariantProps<typeof badge> {
  children: ReactNode;
  icon?: ReactNode;
  className?: string;
}

export function Badge({ children, icon, tone, size, outlined, className }: BadgeProps) {
  return (
    <span className={cn(badge({ tone, size, outlined }), className)}>
      {icon}
      {children}
    </span>
  );
}

/** A badge with a pulsing dot, for "this is happening right now". */
export function LiveBadge({
  children,
  tone = "success",
  className,
}: {
  children: ReactNode;
  tone?: BadgeProps["tone"];
  className?: string;
}) {
  return (
    <Badge tone={tone} className={className}>
      <span className="relative flex size-1.5">
        <span className="absolute inline-flex size-full animate-ping rounded-full bg-current opacity-75" />
        <span className="relative inline-flex size-1.5 rounded-full bg-current" />
      </span>
      {children}
    </Badge>
  );
}
