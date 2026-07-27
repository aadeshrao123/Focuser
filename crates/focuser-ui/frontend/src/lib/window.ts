/**
 * Window controls for the custom title bar.
 *
 * Imported lazily so the browser harness never pulls in the Tauri window API —
 * callers guard on `isTauri()` and these become no-ops if one slips through.
 */

async function currentWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export async function minimizeWindow() {
  (await currentWindow()).minimize();
}

export async function toggleMaximize() {
  (await currentWindow()).toggleMaximize();
}

export async function isMaximized(): Promise<boolean> {
  return (await currentWindow()).isMaximized();
}

/**
 * Closing hides the window instead of exiting.
 *
 * A blocker that quits when you close its window is a blocker you can escape
 * with Alt+F4. It keeps running in the tray; quitting is a tray-menu decision.
 */
export async function closeWindow() {
  (await currentWindow()).hide();
}

/** Subscribe to resize events. Returns an unsubscribe function. */
export function onResized(handler: () => void): () => void {
  let dispose: (() => void) | undefined;
  let cancelled = false;

  currentWindow().then(async (win) => {
    const unlisten = await win.onResized(handler);
    if (cancelled) unlisten();
    else dispose = unlisten;
  });

  return () => {
    cancelled = true;
    dispose?.();
  };
}
