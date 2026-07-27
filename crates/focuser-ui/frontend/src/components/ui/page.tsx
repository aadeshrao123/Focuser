import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * The column every page sits in.
 *
 * Pages used to pick their own `max-w-*`, which meant a maximised window left a
 * wide band of empty canvas on both sides and each page was empty by a
 * different amount. One shell, and `wide` for the pages that genuinely have
 * something to fill it with.
 */
export function Page({
  children,
  wide,
  className,
}: {
  children: ReactNode;
  wide?: boolean;
  className?: string;
}) {
  return (
    <div className={cn("mx-auto w-full px-8 py-7", wide ? "max-w-[100rem]" : "max-w-6xl", className)}>
      {children}
    </div>
  );
}
