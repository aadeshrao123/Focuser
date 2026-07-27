import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";
import { isTauri } from "@/lib/transport";
import { cn } from "@/lib/utils";
import { closeWindow, isMaximized, minimizeWindow, onResized, toggleMaximize } from "@/lib/window";

/**
 * The window's own title bar, drawn by the app rather than the OS.
 *
 * Absent in the browser harness, where there is no window to drag or close.
 * The bar itself is the drag region; the buttons opt out so a click on one is
 * not swallowed by the drag.
 */
export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    isMaximized().then(setMaximized);
    return onResized(() => isMaximized().then(setMaximized));
  }, []);

  if (!isTauri()) return null;

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-between border-border/60 border-b bg-deep/80 pl-3 backdrop-blur"
    >
      <div data-tauri-drag-region className="flex items-center gap-2">
        <Mark />
        <span data-tauri-drag-region className="font-medium text-foreground/90 text-xs">
          Focuser
        </span>
      </div>

      <div className="flex h-full">
        <ControlButton label="Minimise" onClick={minimizeWindow}>
          <Minus className="size-3.5" />
        </ControlButton>
        <ControlButton label={maximized ? "Restore" : "Maximise"} onClick={() => toggleMaximize()}>
          {maximized ? <Copy className="size-3" /> : <Square className="size-3" />}
        </ControlButton>
        <ControlButton label="Close" destructive onClick={closeWindow}>
          <X className="size-3.5" />
        </ControlButton>
      </div>
    </header>
  );
}

function ControlButton({
  label,
  onClick,
  destructive,
  children,
}: {
  label: string;
  onClick: () => void;
  destructive?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "inline-flex h-full w-11 items-center justify-center text-muted-foreground transition-colors",
        "hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        destructive ? "hover:bg-destructive hover:text-white" : "hover:bg-hover",
      )}
    >
      {children}
    </button>
  );
}

/** The hourglass from the app icon, small enough to read at 14px. */
function Mark() {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden
      className="size-3.5 text-primary"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 3h12M6 21h12M8 3v4a4 4 0 0 0 4 4 4 4 0 0 0 4-4V3M8 21v-4a4 4 0 0 1 4-4 4 4 0 0 1 4 4v4" />
    </svg>
  );
}
