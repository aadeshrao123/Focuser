import { BrowserStatusList } from "@/components/browser-status";
import { ConfigTransfer } from "@/components/config-transfer";
import { SettingRow, SettingsSection } from "@/components/setting-row";
import { Card, PageHeader } from "@/components/ui/card";
import { ConfirmButton } from "@/components/ui/confirm-button";
import { InlineError } from "@/components/ui/feedback";
import { NumberField } from "@/components/ui/number-field";
import { Switch } from "@/components/ui/switch";
import { UpdateCheck } from "@/components/update-check";
import { useAutostart } from "@/lib/autostart";
import {
  useAppVersion,
  useDeleteAllData,
  useResetSettings,
  useSetStatsRetention,
  useStatsRetention,
} from "@/lib/commands";
import {
  MAX_RETENTION_DAYS,
  SETTING_KEYS,
  useBooleanSetting,
  useNumberSetting,
} from "@/lib/settings";

export function Settings() {
  const autostart = useAutostart();
  const enforceBrowsers = useBooleanSetting(SETTING_KEYS.blockUnsupportedBrowsers, true);
  const gracePeriod = useNumberSetting(SETTING_KEYS.extensionGracePeriod, 60);

  const retention = useStatsRetention();
  const setRetention = useSetStatsRetention();
  const reset = useResetSettings();
  const deleteAll = useDeleteAllData();
  const version = useAppVersion();

  return (
    <div className="max-w-3xl p-8">
      <PageHeader title="Settings" description="How Focuser behaves on this machine." />

      <SettingsSection title="Startup">
        <SettingRow
          label="Launch at login"
          description={
            autostart.supported
              ? "Focuser starts with your computer, so blocks are in place before you can talk yourself out of them."
              : "Only available in the desktop app."
          }
          control={
            <Switch
              checked={autostart.value}
              onCheckedChange={autostart.set}
              disabled={!autostart.supported || autostart.isPending || autostart.isSaving}
              aria-label="Launch at login"
            />
          }
        />
      </SettingsSection>

      <SettingsSection
        title="Browsers"
        description="The extension blocks pages properly. Without it, only whole domains can be stopped."
      >
        <SettingRow
          label="Close browsers without the extension"
          description="Applies while a block list is active."
          control={
            <Switch
              checked={enforceBrowsers.value}
              onCheckedChange={enforceBrowsers.set}
              disabled={enforceBrowsers.isPending || enforceBrowsers.isSaving}
              aria-label="Close browsers without the extension"
            />
          }
        />
        <SettingRow
          label="Grace period"
          htmlFor="grace-period"
          description="How long to wait before closing one, so there is time to install it."
          control={
            <NumberField
              id="grace-period"
              value={gracePeriod.value}
              onCommit={gracePeriod.set}
              min={5}
              max={3600}
              suffix="seconds"
              disabled={!enforceBrowsers.value || gracePeriod.isPending}
            />
          }
        />
      </SettingsSection>

      <section className="mb-6">
        <h2 className="font-medium text-foreground text-sm">Extension</h2>
        <p className="mt-1 text-muted-foreground text-sm">
          Where the Focuser extension is installed.
        </p>
        <Card className="mt-3" padding="none">
          <BrowserStatusList />
        </Card>
      </section>

      <SettingsSection title="Data">
        <SettingRow
          label="Keep statistics for"
          htmlFor="retention"
          description="Older records are removed automatically."
          control={
            <NumberField
              id="retention"
              value={retention.data ?? 30}
              onCommit={(days) => setRetention.mutate(days)}
              min={1}
              max={MAX_RETENTION_DAYS}
              suffix="days"
              disabled={retention.isPending}
            />
          }
        />
        <SettingRow
          label="Block lists file"
          description="Export every block list to a file, or replace them from one. Statistics and settings are not included."
          control={<ConfigTransfer />}
        />
        <SettingRow
          label="Reset settings"
          description="Puts everything on this page back to its default. Block lists and statistics are untouched."
          control={
            <ConfirmButton variant="outline" size="sm" onConfirm={() => reset.mutate()}>
              Reset
            </ConfirmButton>
          }
        />
        <SettingRow
          label="Delete everything"
          description="Block lists, rules, schedules, statistics and settings. This cannot be undone."
          control={
            <ConfirmButton
              variant="outline"
              size="sm"
              confirmLabel="Click again to delete everything"
              onConfirm={() => deleteAll.mutate()}
              disabled={deleteAll.isPending}
            >
              Delete all data
            </ConfirmButton>
          }
        />
      </SettingsSection>

      <SettingsSection title="About">
        <SettingRow label="Version" control={<Version value={version.data} />} />
        <SettingRow label="Updates" control={<UpdateCheck />} />
      </SettingsSection>

      <InlineError
        error={
          autostart.error ??
          enforceBrowsers.error ??
          gracePeriod.error ??
          setRetention.error ??
          reset.error ??
          deleteAll.error
        }
      />
    </div>
  );
}

function Version({ value }: { value?: string }) {
  return (
    <span className="text-muted-foreground text-sm tabular-nums">
      {value ? `Focuser ${value}` : "—"}
    </span>
  );
}
