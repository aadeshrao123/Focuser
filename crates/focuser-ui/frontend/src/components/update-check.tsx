import { Button } from "@/components/ui/button";
import { isTauri } from "@/lib/native";
import { useInstallUpdate, useUpdate } from "@/lib/updates";
import { m } from "@/paraglide/messages.js";

/**
 * Reads the same query as the sidebar badge, so arriving from that badge does
 * not ask you to check for the thing you were just told about.
 */
export function UpdateCheck() {
  const update = useUpdate();
  const install = useInstallUpdate();

  if (!isTauri()) {
    return <span className="text-faint-foreground text-sm">{m.update_desktop_only()}</span>;
  }

  const available = update.data?.available === true;
  const version = update.data?.version;
  const checking = update.isFetching;
  // Show the updater's own message: "signature mismatch" is worth reading.
  const failure = install.error ?? update.error;

  return (
    <div className="flex items-center gap-3">
      {!available && update.isSuccess && !checking && (
        <span className="text-muted-foreground text-sm">{m.update_up_to_date()}</span>
      )}
      {available && (
        <span className="text-success text-sm">
          {version ? m.update_version_ready({ version }) : m.update_available()}
        </span>
      )}
      {failure && (
        <span role="alert" className="max-w-64 truncate text-destructive text-sm">
          {String(failure)}
        </span>
      )}

      {available ? (
        <Button size="sm" onClick={() => install.mutate()} disabled={install.isPending}>
          {install.isPending ? m.update_installing() : m.update_install_restart()}
        </Button>
      ) : (
        <Button
          variant="outline"
          size="sm"
          onClick={() => update.refetch()}
          disabled={checking}
          data-update-check
        >
          {checking ? m.update_checking() : m.update_check()}
        </Button>
      )}
    </div>
  );
}
