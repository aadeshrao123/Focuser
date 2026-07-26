import { type VariantProps, cva } from "class-variance-authority";
import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { cn } from "@/lib/utils";

/**
 * Button variants.
 *
 * Adding a new look means adding a case here — never a one-off `className` at
 * the call site. That is what keeps buttons consistent as the app grows: there
 * is one place to see every button style that exists, and one place to change
 * them all.
 *
 * `variant` is the shape/emphasis. `tone` recolours the destructive/success
 * intent independently, so you get e.g. a *ghost* button that is still clearly
 * destructive without needing a `ghost-destructive` combination variant.
 */
const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded-md font-medium transition-colors",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
    "disabled:pointer-events-none disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0",
  ],
  {
    variants: {
      variant: {
        solid: "bg-primary text-primary-foreground hover:bg-primary-hover",
        soft: "bg-primary-dim text-foreground hover:bg-hover",
        outline: "border border-border-strong bg-transparent text-foreground hover:bg-hover",
        ghost: "bg-transparent text-muted-foreground hover:bg-hover hover:text-foreground",
        link: "bg-transparent text-primary underline-offset-4 hover:underline",
      },
      tone: {
        default: "",
        destructive: "",
        success: "",
      },
      size: {
        sm: "h-8 px-3 text-xs [&_svg]:size-3.5",
        md: "h-9 px-4 text-sm [&_svg]:size-4",
        lg: "h-11 px-6 text-base [&_svg]:size-5",
        icon: "size-9 p-0 [&_svg]:size-4",
      },
      full: {
        true: "w-full",
        false: "",
      },
    },

    // Tone only changes colour, so it composes with each variant rather than
    // multiplying into variant×tone cases.
    compoundVariants: [
      {
        variant: "solid",
        tone: "destructive",
        class: "bg-destructive text-deep hover:brightness-110",
      },
      { variant: "solid", tone: "success", class: "bg-success text-deep hover:brightness-110" },
      { variant: "outline", tone: "destructive", class: "text-destructive hover:bg-destructive/10" },
      { variant: "outline", tone: "success", class: "text-success hover:bg-success/10" },
      { variant: "ghost", tone: "destructive", class: "text-destructive hover:bg-destructive/10" },
      { variant: "ghost", tone: "success", class: "text-success hover:bg-success/10" },
      { variant: "soft", tone: "destructive", class: "bg-destructive/15 text-destructive" },
      { variant: "soft", tone: "success", class: "bg-success/15 text-success" },
      { variant: "link", tone: "destructive", class: "text-destructive" },
      { variant: "link", tone: "success", class: "text-success" },
    ],

    defaultVariants: {
      variant: "solid",
      tone: "default",
      size: "md",
      full: false,
    },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  ref?: Ref<HTMLButtonElement>;
  /** Rendered before the label. */
  icon?: ReactNode;
}

export function Button({
  className,
  variant,
  tone,
  size,
  full,
  icon,
  children,
  type = "button",
  ref,
  ...props
}: ButtonProps) {
  return (
    <button
      ref={ref}
      // Defaulting to "button": inside a <form>, HTML's default of "submit"
      // causes accidental submits, which is a classic source of lost input.
      type={type}
      className={cn(buttonVariants({ variant, tone, size, full }), className)}
      {...props}
    >
      {icon}
      {children}
    </button>
  );
}

export { buttonVariants };
