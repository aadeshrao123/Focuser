import { SettingRow, SettingsSection } from "@/components/setting-row";
import { PageHeader } from "@/components/ui/card";
import { ConfirmButton } from "@/components/ui/confirm-button";
import { InlineError } from "@/components/ui/feedback";
import { NumberField } from "@/components/ui/number-field";
import { Switch } from "@/components/ui/switch";
import { useAutostart } from "@/lib/autostart";
import { useResetSettings, useSetStatsRetention, useStatsRetention } from "@/lib/commands";
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
          label="Reset settings"
          description="Puts everything on this page back to its default. Block lists and statistics are untouched."
          control={
            <ConfirmButton variant="outline" size="sm" onConfirm={() => reset.mutate()}>
              Reset
            </ConfirmButton>
          }
        />
      </SettingsSection>

      <InlineError
        error={
          autostart.error ??
          enforceBrowsers.error ??
          gracePeriod.error ??
          setRetention.error ??
          reset.error
        }
      />
    </div>
  );
}
