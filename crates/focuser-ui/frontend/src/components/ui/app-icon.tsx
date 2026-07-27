import { cn } from "@/lib/utils";

/**
 * The product mark, from the same PNG the installer and tray use.
 *
 * Served from `public/` rather than inlined so there is exactly one copy of the
 * icon in the repo — a hand-drawn SVG stand-in had already drifted into a
 * completely different shape from the real one.
 */
export function AppIcon({ className }: { className?: string }) {
  return (
    <img
      src="icon.png"
      alt=""
      aria-hidden
      draggable={false}
      className={cn("size-5 select-none", className)}
    />
  );
}
