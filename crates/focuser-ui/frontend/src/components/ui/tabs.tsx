import { cn } from "@/lib/utils";

export interface TabItem<T extends string> {
  id: T;
  label: string;
  /** Shown as a count pill beside the label. */
  count?: number;
}

/**
 * Segmented tabs.
 *
 * The active tab is a filled pill rather than an underline: at this size an
 * underline is a two-pixel cue, and the pill also survives a busy background.
 */
export function Tabs<T extends string>({
  value,
  onChange,
  items,
  className,
}: {
  value: T;
  onChange: (id: T) => void;
  items: readonly TabItem<T>[];
  className?: string;
}) {
  return (
    <div
      role="tablist"
      className={cn(
        "inline-flex items-center gap-1 rounded-lg border border-border bg-surface/70 p-1",
        className,
      )}
    >
      {items.map((item) => {
        const active = item.id === value;
        return (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(item.id)}
            className={cn(
              "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium text-sm",
              "transition-colors duration-150",
              "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
              active
                ? "bg-elevated text-foreground shadow-(--shadow-depth-sm)"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {item.label}
            {item.count !== undefined && (
              <span
                className={cn(
                  "rounded-full px-1.5 py-px text-[0.6875rem] tabular-nums",
                  active ? "bg-primary-dim text-primary" : "bg-elevated text-faint-foreground",
                )}
              >
                {item.count}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
