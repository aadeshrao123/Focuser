/**
 * The chosen interface language.
 *
 * Two places remember it, on purpose. Paraglide keeps it in `localStorage` so a
 * reload paints the right language immediately instead of flashing English
 * while a query resolves. The settings table keeps it too, so it survives a
 * cleared cache, travels with an exported configuration, and is readable by
 * Rust for the parts of the UI a webview does not draw.
 *
 * The settings table wins on startup, since it is the durable one.
 */

import { useEffect } from "react";
import { getLocale, type Locale, locales, setLocale } from "@/paraglide/runtime.js";
import { useSetSetting, useSetting } from "./commands";
import { SETTING_KEYS } from "./settings";

/** English names are deliberate: a language picker lists languages, not translations. */
export const LANGUAGE_NAMES: Record<string, string> = {
  en: "English",
  es: "Español",
};

export function isLocale(value: string | null | undefined): value is Locale {
  return value != null && (locales as readonly string[]).includes(value);
}

/**
 * `reload: false` because a desktop app switching language should not blank
 * itself and start over. React re-renders and every message function returns
 * the new language on the next pass.
 */
function apply(locale: Locale) {
  if (getLocale() !== locale) setLocale(locale, { reload: false });
}

/**
 * Applies the saved language. Call once, high up.
 *
 * The Settings page is not enough: someone who opens the app on the Dashboard
 * never mounts it, so the choice would sit in the database being ignored. This
 * belongs in the layout that wraps every route.
 */
export function useApplySavedLanguage() {
  const saved = useSetting(SETTING_KEYS.language, getLocale()).data;

  // The stored value is the durable one, so it corrects localStorage rather
  // than the other way round.
  useEffect(() => {
    if (isLocale(saved)) apply(saved);
  }, [saved]);
}

export function useLanguage() {
  const query = useSetting(SETTING_KEYS.language, getLocale());
  const save = useSetSetting();
  const saved = query.data;

  useApplySavedLanguage();

  return {
    value: isLocale(saved) ? saved : getLocale(),
    isPending: query.isPending,
    isSaving: save.isPending,
    error: query.error ?? save.error,
    options: locales.map((code) => ({ value: code, label: LANGUAGE_NAMES[code] ?? code })),
    set: (next: Locale) => {
      apply(next);
      save.mutate({ key: SETTING_KEYS.language, value: next });
    },
  };
}
