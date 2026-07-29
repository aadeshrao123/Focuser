import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router-dom";
import { BrowserStatusList } from "@/components/browser-status";
import { ConfigTransfer } from "@/components/config-transfer";
import { SettingRow, SettingsSection } from "@/components/setting-row";
import { PageHeader } from "@/components/ui/card";
import { ConfirmButton } from "@/components/ui/confirm-button";
import { InlineError } from "@/components/ui/feedback";
import { NumberField } from "@/components/ui/number-field";
import { Page } from "@/components/ui/page";
import { Select } from "@/components/ui/select";
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
import { useLanguage } from "@/lib/language";
import {
  MAX_RETENTION_DAYS,
  SETTING_KEYS,
  useBooleanSetting,
  useNumberSetting,
} from "@/lib/settings";
import { m } from "@/paraglide/messages.js";

export function Settings() {
  const autostart = useAutostart();
  const enforceBrowsers = useBooleanSetting(SETTING_KEYS.blockUnsupportedBrowsers, true);
  const gracePeriod = useNumberSetting(SETTING_KEYS.extensionGracePeriod, 60);
  const language = useLanguage();

  const retention = useStatsRetention();
  const setRetention = useSetStatsRetention();
  const reset = useResetSettings();
  const deleteAll = useDeleteAllData();
  const version = useAppVersion();

  // The sidebar badge links here promising the update button, so find it.
  const [params] = useSearchParams();
  const highlightUpdates = params.get("highlight") === "updates";
  const updatesRow = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (highlightUpdates) updatesRow.current?.scrollIntoView({ block: "center" });
  }, [highlightUpdates]);

  return (
    // One column, not two. Splitting settings left/right meant a setting's
    // position on the page carried no meaning — you had to scan both sides.
    <Page>
      <PageHeader title={m.settings_title()} description={m.settings_description()} />

      <SettingsSection title={m.settings_section_startup()}>
        <SettingRow
          label={m.settings_autostart()}
          description={
            autostart.supported
              ? m.settings_autostart_description()
              : m.settings_autostart_unsupported()
          }
          control={
            <Switch
              checked={autostart.value}
              onCheckedChange={autostart.set}
              disabled={!autostart.supported || autostart.isPending || autostart.isSaving}
              aria-label={m.settings_autostart()}
            />
          }
        />
      </SettingsSection>

      <SettingsSection
        title={m.settings_section_browsers()}
        description={m.settings_browsers_description()}
      >
        <SettingRow
          label={m.settings_close_browsers()}
          description={m.settings_close_browsers_description()}
          control={
            <Switch
              checked={enforceBrowsers.value}
              onCheckedChange={enforceBrowsers.set}
              disabled={enforceBrowsers.isPending || enforceBrowsers.isSaving}
              aria-label={m.settings_close_browsers()}
            />
          }
        />
        <SettingRow
          label={m.settings_grace_period()}
          htmlFor="grace-period"
          description={m.settings_grace_period_description()}
          control={
            <NumberField
              id="grace-period"
              value={gracePeriod.value}
              onCommit={gracePeriod.set}
              min={5}
              max={3600}
              step={5}
              suffix={m.settings_seconds_suffix()}
              disabled={!enforceBrowsers.value || gracePeriod.isPending}
            />
          }
        />
      </SettingsSection>

      <SettingsSection
        title={m.settings_section_extension()}
        description={m.settings_extension_description()}
        flush
      >
        <BrowserStatusList />
      </SettingsSection>

      <SettingsSection title={m.settings_section_data()}>
        <SettingRow
          label={m.settings_retention()}
          htmlFor="retention"
          description={m.settings_retention_description()}
          control={
            <NumberField
              id="retention"
              value={retention.data ?? 30}
              onCommit={(days) => setRetention.mutate(days)}
              min={1}
              max={MAX_RETENTION_DAYS}
              suffix={m.settings_days_suffix()}
              disabled={retention.isPending}
            />
          }
        />
        <SettingRow
          label={m.settings_config_file()}
          description={m.settings_config_file_description()}
          control={<ConfigTransfer />}
        />
        <SettingRow
          label={m.settings_reset()}
          description={m.settings_reset_description()}
          control={
            <ConfirmButton variant="outline" size="sm" onConfirm={() => reset.mutate()}>
              {m.settings_reset_action()}
            </ConfirmButton>
          }
        />
        <SettingRow
          label={m.settings_delete_all()}
          description={m.settings_delete_all_description()}
          control={
            <ConfirmButton
              variant="outline"
              size="sm"
              confirmLabel={m.config_confirm_delete()}
              onConfirm={() => deleteAll.mutate()}
              disabled={deleteAll.isPending}
            >
              {m.settings_delete_all_action()}
            </ConfirmButton>
          }
        />
      </SettingsSection>

      <SettingsSection title={m.settings_section_language()}>
        <SettingRow
          label={m.settings_language()}
          htmlFor="language"
          description={m.settings_language_description()}
          control={
            <Select
              id="language"
              value={language.value}
              onValueChange={language.set}
              options={language.options}
              size="sm"
              disabled={language.isPending}
              aria-label={m.settings_language()}
            />
          }
        />
      </SettingsSection>

      <SettingsSection title={m.settings_section_about()}>
        <SettingRow label={m.settings_version()} control={<Version value={version.data} />} />
        <div ref={updatesRow}>
          <SettingRow
            label={m.settings_updates()}
            control={<UpdateCheck />}
            highlight={highlightUpdates}
          />
        </div>
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
    </Page>
  );
}

function Version({ value }: { value?: string }) {
  return (
    <span className="text-muted-foreground text-sm tabular-nums">
      {value ? `Focuser ${value}` : "—"}
    </span>
  );
}
