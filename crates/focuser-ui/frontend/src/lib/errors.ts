/**
 * Turning a failed command into something worth reading.
 *
 * Rust returns a stable `code` alongside English prose. The code is what gets
 * translated, because the prose is written for whoever is reading a log, not
 * for whoever is looking at the screen.
 *
 * Codes with no entry here fall back to the Rust message. That is deliberate:
 * `validation`, `core` and `internal` carry detail in their text — which field
 * was wrong, which file could not be opened — and a generic translated
 * sentence would throw that away. Better untranslated and specific than
 * translated and useless.
 */

import { m } from "@/paraglide/messages.js";
import { CommandError } from "./transport";

const BY_CODE: Record<string, () => string> = {
  block_list_not_found: m.error_block_list_not_found,
  rule_not_found: m.error_rule_not_found,
  allowance_not_found: m.error_allowance_not_found,
  protected: m.error_protected,
  unsupported: m.error_unsupported,
  transport: m.error_transport,
};

export function errorMessage(error: Error): string {
  if (error instanceof CommandError) {
    const translated = BY_CODE[error.code];
    if (translated) return translated();
  }
  return error.message;
}
