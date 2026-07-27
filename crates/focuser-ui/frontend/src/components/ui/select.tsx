import * as RadixSelect from "@radix-ui/react-select";
import { type VariantProps, cva } from "class-variance-authority";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

const trigger = cva(
  [
    "group inline-flex items-center justify-between gap-2 rounded-md border bg-elevated text-foreground",
    "border-border shadow-(--shadow-depth-sm) transition-colors",
    "hover:border-border-strong hover:bg-hover",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
    "disabled:cursor-not-allowed disabled:opacity-50",
    "data-[state=open]:border-primary/60 data-[state=open]:bg-hover",
  ],
  {
    variants: {
      size: {
        sm: "h-8 px-2.5 text-xs",
        md: "h-9 px-3 text-sm",
      },
    },
    defaultVariants: { size: "md" },
  },
);

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  /** Optional second line, for when the label alone is ambiguous. */
  hint?: string;
}

export interface SelectProps<T extends string> extends VariantProps<typeof trigger> {
  value: T;
  onValueChange: (value: T) => void;
  options: readonly SelectOption<T>[];
  disabled?: boolean;
  placeholder?: string;
  id?: string;
  "aria-label"?: string;
  className?: string;
}

/**
 * Dropdown built on Radix.
 *
 * This replaced a styled native `<select>`, whose popup is drawn by the OS and
 * cannot be themed — a bright grey list falling out of a dark window. Radix
 * renders the list itself, so it matches the app, animates, and can carry hints
 * and check marks. Keyboard navigation, type-ahead and focus management come
 * with it rather than being re-implemented.
 */
export function Select<T extends string>({
  value,
  onValueChange,
  options,
  disabled,
  placeholder,
  size,
  className,
  id,
  ...props
}: SelectProps<T>) {
  return (
    <RadixSelect.Root value={value} onValueChange={(v) => onValueChange(v as T)} disabled={disabled}>
      <RadixSelect.Trigger
        id={id}
        aria-label={props["aria-label"]}
        className={cn(trigger({ size }), className)}
      >
        <RadixSelect.Value placeholder={placeholder} />
        <RadixSelect.Icon asChild>
          <ChevronDown className="size-4 shrink-0 text-muted-foreground transition-transform duration-200 group-data-[state=open]:rotate-180" />
        </RadixSelect.Icon>
      </RadixSelect.Trigger>

      <RadixSelect.Portal>
        <RadixSelect.Content
          position="popper"
          sideOffset={6}
          className={cn(
            "z-50 min-w-(--radix-select-trigger-width) overflow-hidden rounded-lg border border-border-strong",
            "bg-elevated/95 shadow-(--shadow-depth-lg) backdrop-blur-xl",
            "data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=open]:animate-in",
            "data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=closed]:animate-out",
            "data-[side=bottom]:slide-in-from-top-1 data-[side=top]:slide-in-from-bottom-1",
          )}
        >
          <RadixSelect.Viewport className="max-h-72 p-1">
            {options.map((option) => (
              <RadixSelect.Item
                key={option.value}
                value={option.value}
                className={cn(
                  "relative flex cursor-pointer select-none items-center gap-2 rounded-md py-1.5 pr-2 pl-8",
                  "text-foreground text-sm outline-none transition-colors",
                  "data-[highlighted]:bg-primary-dim data-[state=checked]:font-medium",
                )}
              >
                <RadixSelect.ItemIndicator className="absolute left-2 flex items-center">
                  <Check className="size-3.5 text-primary" />
                </RadixSelect.ItemIndicator>
                <div className="min-w-0">
                  <RadixSelect.ItemText>{option.label}</RadixSelect.ItemText>
                  {option.hint && (
                    <p className="truncate text-faint-foreground text-xs">{option.hint}</p>
                  )}
                </div>
              </RadixSelect.Item>
            ))}
          </RadixSelect.Viewport>
        </RadixSelect.Content>
      </RadixSelect.Portal>
    </RadixSelect.Root>
  );
}
