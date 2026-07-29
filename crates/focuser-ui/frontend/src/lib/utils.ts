import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge conditional class names, resolving Tailwind conflicts. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// There used to be a `count(n, "site")` helper here that pluralised by adding
// an "s". It was quietly English-only — the last four calls to it were still
// printing "2 sites" in Japanese — so counted nouns now live in the catalogue as
// plural messages (`count_sites`, `count_apps`, …) and go through
// `Intl.PluralRules` like everything else.
