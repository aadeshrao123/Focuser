import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Card } from "./ui/card";

/**
 * A titled group of settings.
 *
 * `flush` is for children that already draw their own row separators, so the
 * card doesn't add a second set on top.
 */
export function SettingsSection({
  title,
  description,
  flush,
  children,
}: {
  title: string;
  description?: string;
  flush?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="mb-7">
      <h2 className="font-semibold text-foreground text-sm">{title}</h2>
      {description && <p className="mt-1 text-muted-foreground text-sm">{description}</p>}
      <Card
        className={cn("mt-3", !flush && "divide-y divide-border")}
        padding="none"
        elevation="raised"
      >
        {children}
      </Card>
    </section>
  );
}

/** One labelled control. The label is a real `<label>` when the control has an id. */
export function SettingRow({
  label,
  description,
  htmlFor,
  control,
  highlight,
}: {
  label: string;
  description?: string;
  htmlFor?: string;
  control: ReactNode;
  /** For arrivals from a link that promised this row specifically. */
  highlight?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-8 px-5 py-4 transition-colors",
        highlight ? "bg-primary/10 ring-1 ring-primary/40 ring-inset" : "hover:bg-hover/40",
      )}
    >
      <div className="min-w-0">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="font-medium text-foreground text-sm">
            {label}
          </label>
        ) : (
          <p className="font-medium text-foreground text-sm">{label}</p>
        )}
        {description && (
          <p className="mt-1 max-w-prose text-muted-foreground text-sm leading-relaxed">
            {description}
          </p>
        )}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}
