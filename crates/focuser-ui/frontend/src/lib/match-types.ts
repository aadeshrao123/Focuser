import type { AppMatchType, ExceptionType, WebsiteMatchType } from "@/bindings";

/** Externally-tagged Rust enums arrive as `{ Variant: value }` or a bare string. */
function tagged(v: unknown): { label: string; value: string } {
  if (typeof v === "string") return { label: v, value: "" };
  const [key, value] = Object.entries(v as Record<string, string>).find(([, x]) => x != null) ?? [
    "Unknown",
    "",
  ];
  return { label: key, value: String(value) };
}

const WEBSITE_LABELS: Record<string, string> = {
  Domain: "Domain",
  Keyword: "Keyword",
  Wildcard: "Wildcard",
  UrlPath: "URL path",
  EntireInternet: "Entire internet",
};

const APP_LABELS: Record<string, string> = {
  ExecutableName: "Executable",
  ExecutablePath: "Path",
  WindowTitle: "Window title",
  BundleId: "Bundle ID",
};

const EXCEPTION_LABELS: Record<string, string> = {
  Domain: "Domain",
  Wildcard: "Wildcard",
  LocalFiles: "Local files",
};

export function describeWebsite(m: WebsiteMatchType) {
  const { label, value } = tagged(m);
  return { kind: WEBSITE_LABELS[label] ?? label, value };
}

export function describeApp(m: AppMatchType) {
  const { label, value } = tagged(m);
  return { kind: APP_LABELS[label] ?? label, value };
}

export function describeException(m: ExceptionType) {
  const { label, value } = tagged(m);
  return { kind: EXCEPTION_LABELS[label] ?? label, value };
}

export const WEBSITE_KINDS = [
  { value: "Domain", label: "Domain" },
  { value: "Keyword", label: "Keyword" },
  { value: "Wildcard", label: "Wildcard" },
  { value: "UrlPath", label: "URL path" },
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
