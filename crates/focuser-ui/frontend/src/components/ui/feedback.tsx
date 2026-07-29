import { AlertTriangle, Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "./button";
import { errorMessage } from "@/lib/errors";
import { m } from "@/paraglide/messages.js";

/**
 * Loading / error / content for any backend-backed view.
 * Shared so a page can't forget its error branch and show "nothing" when it's broken.
 */
export function QueryState({
  isPending,
  error,
  onRetry,
  isRetrying,
  children,
}: {
  isPending: boolean;
  error: Error | null;
  onRetry?: () => void;
  isRetrying?: boolean;
  children: ReactNode;
}) {
  if (isPending) {
    return (
      <div
        className="flex items-center gap-2 py-8 text-muted-foreground text-sm"
        role="status"
        aria-live="polite"
      >
        <Loader2 aria-hidden className="size-4 animate-spin" />
        {m.common_loading()}
      </div>
    );
  }

  if (error) {
    return (
      <div
        role="alert"
        className="rounded-lg border border-destructive/40 bg-surface p-4"
      >
        <p className="flex items-center gap-2 font-medium text-destructive text-sm">
          <AlertTriangle aria-hidden className="size-4" />
          {m.common_error()}
        </p>
        <p className="mt-1 text-muted-foreground text-sm">{error.message}</p>
        {onRetry && (
          <Button
            variant="outline"
            size="sm"
            className="mt-3"
            onClick={onRetry}
            disabled={isRetrying}
          >
            {isRetrying ? "Retrying…" : "Retry"}
          </Button>
        )}
      </div>
    );
  }

  return <>{children}</>;
}

/** Inline error for a failed mutation, e.g. a rejected form submission. */
export function InlineError({ error }: { error: Error | null }) {
  if (!error) return null;
  return (
    <p role="alert" className="mt-2 text-destructive text-sm">
      {errorMessage(error)}
    </p>
  );
}
