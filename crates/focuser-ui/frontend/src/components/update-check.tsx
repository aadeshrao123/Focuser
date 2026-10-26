import { Button } from "@/components/ui/button";
import { isTauri } from "@/lib/native";
import { useAuthorityUpdate, useCancelAuthorityUpdate, useInstallUpdate, useUpdate } from "@/lib/updates";
import { m } from "@/paraglide/messages.js";

/**
 * Reads the same query as the sidebar badge, so arriving from that badge does
 * not ask you to check for the thing you were just told about.
 */
export function UpdateCheck() {
  const update = useUpdate();
  const install = useInstallUpdate();
  const authority = useAuthorityUpdate();
  const cancel = useCancelAuthorityUpdate();

  if (!isTauri()) {
    return <span className="text-faint-foreground text-sm">{m.update_desktop_only()}</span>;
  }
  if (authority.data && authority.data.phase !== "legacy") {
    const status = authority.data;
    const canStage = status.phase === "available";
    const canActivate = status.phase === "staged" || status.phase === "deferred";
    const frozen = ["authorized", "replacing", "validating", "rolling_back", "rollback_validating"].includes(status.phase);
    const failure = authority.error ?? install.error ?? cancel.error ?? update.error;
    return (
      <div className="flex flex-wrap items-center gap-3">
        <span role="status" className="text-sm">
          {m.update_authority_state({ phase: status.phase, reason: status.code })}
        </span>
        {status.total > 0 && <progress aria-label={m.update_download_progress()} value={status.downloaded} max={status.total} />}
        {failure && <span role="alert" className="text-destructive text-sm">{String(failure)}</span>}
        {(canStage || canActivate) && <Button size="sm" disabled={install.isPending} onClick={() => install.mutate()}>{canStage ? m.update_stage() : m.update_activate()}</Button>}
        {!status.transaction && <Button size="sm" variant="outline" onClick={() => update.refetch()} disabled={update.isFetching}>{m.update_check()}</Button>}
        {status.transaction && (status.can_cancel ?? !frozen) && !["complete", "rolled_back", "cancelled"].includes(status.phase) && <Button size="sm" variant="outline" disabled={cancel.isPending} onClick={() => cancel.mutate()}>{m.update_cancel_stage()}</Button>}
        {["complete", "rolled_back", "cancelled"].includes(status.phase) && <Button size="sm" variant="outline" onClick={() => update.refetch()} disabled={update.isFetching}>{m.update_check()}</Button>}
      </div>
    );
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
