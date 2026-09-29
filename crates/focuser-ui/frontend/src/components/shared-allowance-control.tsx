import type { BlockList } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { InlineError } from "@/components/ui/feedback";
import { NumberField } from "@/components/ui/number-field";
import { Switch } from "@/components/ui/switch";
import {
  useBlockingHealth,
  useBrowserStatus,
  useConfigureSharedAllowance,
  useProtectionStatus,
  useSharedAllowanceStatus,
} from "@/lib/commands";
import { m } from "@/paraglide/messages.js";

export function SharedAllowanceControl({ list }: { list: BlockList }) {
  const configure = useConfigureSharedAllowance();
  const protection = useProtectionStatus();
  const health = useBlockingHealth();
  const browsers = useBrowserStatus();
  const statuses = useSharedAllowanceStatus();
  const status = statuses.data?.find((s) => s.block_list_id === list.id);
  const config = list.shared_allowance;
  const disabled =
    protection.isPending ||
    protection.isError ||
    configure.isPending ||
    protection.data?.some((p) => p.block_list_id === list.id && p.prevent_modification);
  const scheduled = !!list.schedule?.enabled && !!list.schedule.time_slots.length;
  const save = (minutes: number | null) => configure.mutate({ listId: list.id, minutes });

  return (
    <section className="border-border border-t p-4" aria-label={m.shared_allowance_label()}>
      <div className="flex flex-wrap items-center gap-3">
        <Switch
          checked={!!config}
          disabled={disabled || (!config && !scheduled)}
          onCheckedChange={(on) => save(on ? 30 : null)}
          aria-label={m.shared_allowance_label()}
        />
        <span className="font-medium">{m.shared_allowance_label()}</span>
        {config && (
          <>
            <NumberField
              value={config.minutes}
              min={1}
              max={1440}
              disabled={disabled}
              onCommit={save}
              aria-label={m.shared_allowance_minutes()}
            />
            <span className="text-sm text-muted-foreground">{m.shared_allowance_period()}</span>
          </>
        )}
      </div>
      {!scheduled && (
        <p className="mt-2 text-sm text-muted-foreground">{m.shared_allowance_schedule_hint()}</p>
      )}
      {config && <p className="mt-2 text-sm text-muted-foreground">{m.shared_allowance_hint()}</p>}
      {config &&
        list.websites.some((r) => r.enabled) &&
        !browsers.data?.some((b) => b.extension_connected) && (
          <p className="mt-2 text-sm text-warning">{m.allowances_extension_needed_title()}</p>
        )}
      {config &&
        list.applications.some((r) => r.enabled) &&
        health.data?.app_usage_measurable === false && (
          <p className="mt-2 text-sm text-warning">{m.allowances_wayland_body()}</p>
        )}
      {status?.active && (
        <Badge className="mt-3" tone={status.remaining_secs === 0 ? "warning" : "neutral"}>
          {status.remaining_secs === 0
            ? m.shared_allowance_exhausted()
            : m.shared_allowance_remaining({
                remaining: Math.ceil(status.remaining_secs / 60),
                total: status.limit_secs / 60,
              })}
        </Badge>
      )}
      <InlineError error={configure.error ?? statuses.error} />
    </section>
  );
}
