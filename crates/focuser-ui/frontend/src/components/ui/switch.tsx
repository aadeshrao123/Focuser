import { type VariantProps, cva } from "class-variance-authority";
import type { Ref } from "react";
import { cn } from "@/lib/utils";

const track = cva(
  [
    "relative inline-flex shrink-0 cursor-pointer items-center rounded-full transition-colors",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
    "disabled:cursor-not-allowed disabled:opacity-50",
  ],
  {
    variants: {
      size: {
        sm: "h-5 w-9",
        md: "h-6 w-11",
      },
      checked: {
        true: "bg-primary",
        false: "bg-elevated",
      },
    },
    defaultVariants: { size: "md", checked: false },
  },
);

const knob = cva("pointer-events-none rounded-full bg-white shadow-(--shadow-depth-sm)", {
  variants: {
    size: {
      sm: "size-3.5",
      md: "size-4.5",
    },
  },
  defaultVariants: { size: "md" },
});

export interface SwitchProps extends VariantProps<typeof track> {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  id?: string;
  "aria-label"?: string;
  "aria-describedby"?: string;
  className?: string;
  ref?: Ref<HTMLButtonElement>;
}

/**
 * On/off switch. A real `<button role="switch">` rather than a styled checkbox,
 * so `aria-checked` is what assistive tech reads and there is no hidden input
 * to keep in sync.
 */
export function Switch({
  checked,
  onCheckedChange,
  disabled,
  size,
  className,
  ref,
  ...props
}: SwitchProps) {
  return (
    <button
      ref={ref}
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(track({ size, checked }), className)}
      {...props}
    >
      <span
        className={cn(
          knob({ size }),
          // Same 2px gap either side of the knob: track width minus knob minus 2.
          "translate-x-0.5 transition-transform",
          checked && (size === "sm" ? "translate-x-5" : "translate-x-6"),
        )}
      />
    </button>
  );
}
