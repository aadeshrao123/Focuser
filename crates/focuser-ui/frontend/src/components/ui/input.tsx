import { type VariantProps, cva } from "class-variance-authority";
import type { InputHTMLAttributes, Ref } from "react";
import { cn } from "@/lib/utils";

const inputVariants = cva(
  [
    "w-full rounded-md border bg-surface text-foreground transition-colors",
    "placeholder:text-faint-foreground",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
    "disabled:cursor-not-allowed disabled:opacity-50",
  ],
  {
    variants: {
      size: {
        sm: "h-8 px-2.5 text-xs",
        md: "h-9 px-3 text-sm",
        lg: "h-11 px-4 text-base",
      },
      invalid: {
        true: "border-destructive",
        false: "border-border hover:border-border-strong",
      },
    },
    defaultVariants: { size: "md", invalid: false },
  },
);

export interface InputProps
  // `size` is an HTML attribute (character width) as well as one of our
  // variants; ours wins, so the native one is omitted.
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "size">,
    VariantProps<typeof inputVariants> {
  ref?: Ref<HTMLInputElement>;
}

export function Input({ className, size, invalid, ref, ...props }: InputProps) {
  return (
    <input
      ref={ref}
      // Screen readers need the invalid state announced, not just coloured.
      aria-invalid={invalid || undefined}
      className={cn(inputVariants({ size, invalid }), className)}
      {...props}
    />
  );
}

export { inputVariants };
