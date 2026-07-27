import { ChevronDown } from "lucide-react";
import type { Ref, SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export interface SelectOption<T extends string> {
  value: T;
  label: string;
}

export interface SelectProps<T extends string>
  extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "onChange" | "value" | "size"> {
  value: T;
  onValueChange: (value: T) => void;
  options: readonly SelectOption<T>[];
  size?: "sm" | "md";
  ref?: Ref<HTMLSelectElement>;
}

/**
 * Styled native `<select>` — keyboard nav, type-ahead and a11y come free.
 * If we ever need rich option rows, use shadcn's Radix Select.
 */
export function Select<T extends string>({
  className,
  value,
  onValueChange,
  options,
  size = "md",
  ref,
  ...props
}: SelectProps<T>) {
  return (
    <div className="relative inline-flex">
      <select
        ref={ref}
        value={value}
        onChange={(e) => onValueChange(e.target.value as T)}
        className={cn(
          "w-full appearance-none rounded-md border border-border bg-surface pr-8 text-foreground",
          "transition-colors hover:border-border-strong",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
          "disabled:cursor-not-allowed disabled:opacity-50",
          size === "sm" ? "h-8 pl-2.5 text-xs" : "h-9 pl-3 text-sm",
          className,
        )}
        {...props}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronDown
        aria-hidden
        className="pointer-events-none absolute top-1/2 right-2 size-4 -translate-y-1/2 text-muted-foreground"
      />
    </div>
  );
}
