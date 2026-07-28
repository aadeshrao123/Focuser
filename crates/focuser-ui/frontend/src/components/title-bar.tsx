import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";
import { AppIcon } from "@/components/ui/app-icon";
import { isTauri } from "@/lib/transport";
import { cn } from "@/lib/utils";
import { closeWindow, isMaximized, minimizeWindow, onResized, toggleMaximize } from "@/lib/window";
import { m } from "@/paraglide/messages.js";

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
        <AppIcon className="size-4" />
        <span data-tauri-drag-region className="font-medium text-foreground/90 text-xs">
          Focuser
        </span>
      </div>

      <div className="flex h-full">
        <ControlButton label={m.titlebar_minimise()} onClick={minimizeWindow}>
          <Minus className="size-3.5" />
        </ControlButton>
        <ControlButton label={maximized ? "Restore" : "Maximise"} onClick={() => toggleMaximize()}>
          {maximized ? <Copy className="size-3" /> : <Square className="size-3" />}
        </ControlButton>
        <ControlButton label={m.titlebar_close()} destructive onClick={closeWindow}>
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
