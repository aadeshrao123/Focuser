/**
 * The handful of things the desktop shell does that the command core cannot:
 * file dialogs, the updater, launching a browser.
 *
 * Each one is unavailable in the browser harness, so callers check `isTauri()`
 * and hide or disable the control rather than letting it fail at click time.
 */

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./transport";

export { isTauri };

/**
 * The OS this window runs on. Read off the user agent rather than a Tauri
 * plugin: callers only pick text or styles with it, and the preview harness
 * gets it right for free.
 */
export function hostPlatform(): "windows" | "macos" | "linux" {
  const agent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (agent.includes("Mac")) return "macos";
  if (agent.includes("Windows")) return "windows";
  return "linux";
}

/** Pick an executable to block. Returns its file name, or null if cancelled. */
export function pickApplication(): Promise<string | null> {
  return invoke<string | null>("pick_app_file");
}

/** Pick a configuration file and return its contents, or null if cancelled. */
export function pickConfigurationFile(): Promise<string | null> {
  return invoke<string | null>("pick_import_file");
}

/** Save an exported configuration. Returns the path, or null if cancelled. */
export function saveConfiguration(json: string): Promise<string | null> {
  return invoke<string | null>("save_configuration", { json });
}

/** Pick an audio file for the phase chime. Returns its path, or null if cancelled. */
export function pickSound(): Promise<string | null> {
  return invoke<string | null>("pick_sound_file");
}

/** Play the phase chime once, as currently configured. */
export function previewSound(): Promise<void> {
  return invoke("preview_sound");
}

export function openInBrowser(browser: string, url: string): Promise<void> {
  return invoke("open_browser_url", { browser, url });
}

export interface UpdateCheck {
  available: boolean;
  version?: string;
  body?: string;
}

export function checkForUpdate(): Promise<UpdateCheck> {
  return invoke<UpdateCheck>("check_for_update");
}

export function installUpdate(): Promise<void> {
  return invoke("do_update");
}
