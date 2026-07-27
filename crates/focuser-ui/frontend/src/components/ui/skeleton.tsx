import { cn } from "@/lib/utils";

/**
 * Placeholder shaped like the thing that is loading.
 *
 * Better than a spinner where the layout is known: the page does not jump when
 * the data lands, and the shape tells you what is coming.
 */
export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      aria-hidden
      className={cn("animate-pulse rounded-md bg-elevated/80", className)}
    />
  );
}

/** Matching placeholder for a row of `Stat` cards. */
export function StatGridSkeleton({ count = 4 }: { count?: number }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {Array.from({ length: count }, (_, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders have no identity
          key={i}
          className="rounded-xl border border-border bg-surface p-4"
        >
          <Skeleton className="h-3 w-24" />
          <Skeleton className="mt-3 h-7 w-16" />
        </div>
      ))}
    </div>
  );
}

/** Matching placeholder for a stack of list rows. */
export function ListSkeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div className="flex flex-col gap-2">
      {Array.from({ length: rows }, (_, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders have no identity
          key={i}
          className="flex items-center justify-between rounded-xl border border-border bg-surface px-4 py-3.5"
        >
          <div className="w-full">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="mt-2 h-3 w-24" />
          </div>
          <Skeleton className="size-6 shrink-0 rounded-full" />
        </div>
      ))}
    </div>
  );
}
