import { Toaster as Sonner, toast } from "sonner";

export { toast };

/**
 * Transient confirmations, for actions whose result is otherwise invisible —
 * "exported to …", "imported 3 block lists". Anything the user must act on
 * belongs on the page, not in something that disappears.
 */
export function Toaster() {
  return (
    <Sonner
      position="bottom-right"
      offset={16}
      toastOptions={{
        classNames: {
          toast:
            "!bg-elevated/95 !border-border-strong !text-foreground !shadow-(--shadow-depth-lg) !backdrop-blur-xl !rounded-xl",
          description: "!text-muted-foreground",
          actionButton: "!bg-primary !text-primary-foreground",
          cancelButton: "!bg-hover !text-muted-foreground",
          error: "!text-destructive",
          success: "!text-success",
        },
      }}
    />
  );
}
