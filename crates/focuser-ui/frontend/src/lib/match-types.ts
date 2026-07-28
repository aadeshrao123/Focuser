import type { AppMatchType, ExceptionType, WebsiteMatchType } from "@/bindings";
import { m } from "@/paraglide/messages.js";

/** Externally-tagged Rust enums arrive as `{ Variant: value }` or a bare string. */
function tagged(v: unknown): { label: string; value: string } {
  if (typeof v === "string") return { label: v, value: "" };
  const [key, value] = Object.entries(v as Record<string, string>).find(([, x]) => x != null) ?? [
    "Unknown",
    "",
  ];
  return { label: key, value: String(value) };
}

// Functions, not strings: a module-level lookup of message *values* would
// freeze the locale that was active when this file was first imported.
const WEBSITE_LABELS: Record<string, () => string> = {
  Domain: m.websites_kind_domain,
  Keyword: m.websites_kind_keyword,
  Wildcard: m.websites_kind_wildcard,
  UrlPath: m.websites_kind_url_path,
  EntireInternet: m.websites_kind_entire_internet,
};

const APP_LABELS: Record<string, () => string> = {
  ExecutableName: m.apps_kind_executable_name,
  ExecutablePath: m.apps_kind_executable_path,
  WindowTitle: m.apps_kind_window_title,
  BundleId: m.apps_kind_bundle_id,
};

const EXCEPTION_LABELS: Record<string, () => string> = {
  Domain: m.websites_kind_domain,
  Wildcard: m.websites_kind_wildcard,
  LocalFiles: m.exceptions_kind_local_files,
};

/**
 * `kind` is the Rust variant name and never changes; `label` is what the user
 * reads. They are separate because the rule table picks an icon and a colour by
 * kind, and keying that off translated text would leave every row unstyled in
 * any language but English.
 */
function describe(
  match: unknown,
  labels: Record<string, () => string>,
): { kind: string; label: string; value: string } {
  const { label, value } = tagged(match);
  return { kind: label, label: labels[label]?.() ?? label, value };
}

export function describeWebsite(match: WebsiteMatchType) {
  return describe(match, WEBSITE_LABELS);
}

export function describeApp(match: AppMatchType) {
  return describe(match, APP_LABELS);
}

export function describeException(match: ExceptionType) {
  return describe(match, EXCEPTION_LABELS);
}

/** Bulk import takes a value per line, so the whole-internet rule has no place. */
export const IMPORT_KINDS = [
  { value: "Domain", label: "Domain" },
  { value: "Keyword", label: "Keyword" },
  { value: "Wildcard", label: "Wildcard" },
  { value: "UrlPath", label: "URL path" },
] as const;

export const WEBSITE_KINDS = [
  ...IMPORT_KINDS,
  { value: "EntireInternet", label: "Entire internet" },
] as const;

export const APP_KINDS = [
  { value: "ExecutableName", label: "Executable name" },
  { value: "ExecutablePath", label: "Full path" },
  { value: "WindowTitle", label: "Window title" },
] as const;

export const EXCEPTION_KINDS = [
  { value: "Domain", label: "Domain" },
  { value: "Wildcard", label: "Wildcard" },
] as const;

export type WebsiteKind = (typeof WEBSITE_KINDS)[number]["value"];
export type ImportKind = (typeof IMPORT_KINDS)[number]["value"];
export type AppKind = (typeof APP_KINDS)[number]["value"];
export type ExceptionKind = (typeof EXCEPTION_KINDS)[number]["value"];

// Written out rather than computed: the generated enums use exclusive-key
// intersections, so a `{ [kind]: value }` object doesn't satisfy them.
export function websiteRule(kind: WebsiteKind, value: string): WebsiteMatchType {
  switch (kind) {
    case "Domain":
      return { Domain: value };
    case "Keyword":
      return { Keyword: value };
    case "Wildcard":
      return { Wildcard: value };
    case "UrlPath":
      return { UrlPath: value };
    case "EntireInternet":
      return "EntireInternet";
  }
}

export function appRule(kind: AppKind, value: string): AppMatchType {
  switch (kind) {
    case "ExecutableName":
      return { ExecutableName: value };
    case "ExecutablePath":
      return { ExecutablePath: value };
    case "WindowTitle":
      return { WindowTitle: value };
  }
}

export function exceptionRule(kind: ExceptionKind, value: string): ExceptionType {
  return kind === "Domain" ? { Domain: value } : { Wildcard: value };
}
