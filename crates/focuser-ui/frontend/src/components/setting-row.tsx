import type { ReactNode } from "react";
import { Card } from "./ui/card";

export function SettingsSection({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="mb-6">
      <h2 className="font-medium text-foreground text-sm">{title}</h2>
      {description && <p className="mt-1 text-muted-foreground text-sm">{description}</p>}
      <Card className="mt-3 divide-y divide-border" padding="none">
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
}: {
  label: string;
  description?: string;
  htmlFor?: string;
  control: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3.5">
      <div className="min-w-0">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="font-medium text-foreground text-sm">
            {label}
          </label>
        ) : (
          <p className="font-medium text-foreground text-sm">{label}</p>
        )}
        {description && <p className="mt-0.5 text-muted-foreground text-sm">{description}</p>}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}
