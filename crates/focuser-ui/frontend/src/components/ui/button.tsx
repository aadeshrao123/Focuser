import { Slot } from "@radix-ui/react-slot";
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
    "rounded-md font-medium",
    // Press feedback: a small scale-down reads as physical without moving
    // neighbours, which a translate or a border change would.
    "transition-[background-color,border-color,color,box-shadow,transform] duration-150",
    "active:scale-[0.97]",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
    "disabled:pointer-events-none disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0",
  ],
  {
    variants: {
      variant: {
        solid: [
          "bg-primary text-primary-foreground shadow-(--shadow-depth-sm)",
          "hover:bg-primary-hover hover:shadow-(--shadow-glow)",
          "active:bg-primary-active",
        ],
        soft: "bg-primary-dim text-foreground hover:bg-primary/25",
        outline: [
          "border border-border-strong bg-elevated/60 text-foreground",
          "hover:border-primary/50 hover:bg-hover",
        ],
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
  /**
   * Render the child element with the button's styling instead of a `<button>`.
   * Used for links, which must stay anchors for middle-click and focus order.
   */
  asChild?: boolean;
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
  asChild,
  ref,
  ...props
}: ButtonProps) {
  const Root = asChild ? Slot : "button";

  return (
    <Root
      ref={ref}
      // Defaulting to "button": inside a <form>, HTML's default of "submit"
      // causes accidental submits, which is a classic source of lost input.
      type={asChild ? undefined : type}
      className={cn(buttonVariants({ variant, tone, size, full }), className)}
      {...props}
    >
      {/* Slot forwards to a single child, so a link button wraps its own span. */}
      {asChild ? (
        children
      ) : (
        <>
          {icon}
          {children}
        </>
      )}
    </Root>
  );
}

export { buttonVariants };
