import { useEffect, useState } from "react";
import { Button, type ButtonProps } from "./button";

export interface ConfirmButtonProps extends Omit<ButtonProps, "onClick"> {
  /** Label shown once the button is armed. */
  confirmLabel?: string;
  onConfirm: () => void;
}

/**
 * Two-step destructive action: the first click arms it, the second fires.
 * Disarms itself after a few seconds so a stray click can't sit there loaded.
 */
export function ConfirmButton({
  children,
  confirmLabel = "Click again to confirm",
  onConfirm,
  ...props
}: ConfirmButtonProps) {
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(timer);
  }, [armed]);

  return (
    <Button
      {...props}
      tone="destructive"
      onClick={() => {
        if (armed) onConfirm();
        setArmed(!armed);
      }}
      onBlur={() => setArmed(false)}
    >
      {armed ? confirmLabel : children}
    </Button>
  );
}
