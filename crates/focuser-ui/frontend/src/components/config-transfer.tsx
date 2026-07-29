import { Download, Upload } from "lucide-react";
import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { InlineError } from "@/components/ui/feedback";
import { useExportConfiguration, useImportConfiguration } from "@/lib/commands";
import { isTauri, pickConfigurationFile, saveConfiguration } from "@/lib/native";
import { m } from "@/paraglide/messages.js";

/**
 * Export and import every block list as a JSON document.
 *
 * The document itself comes from the command core; only *where it goes* differs
 * by host — a native save dialog in the desktop app, a download in the browser.
 */
export function ConfigTransfer() {
  const exporter = useExportConfiguration();
  const importer = useImportConfiguration();
  const filePicker = useRef<HTMLInputElement>(null);
  const [note, setNote] = useState<string | null>(null);

  async function exportConfig() {
    setNote(null);
    const json = await exporter.mutateAsync();

    if (isTauri()) {
      const path = await saveConfiguration(json);
      if (path) setNote(m.config_saved_to({ path }));
      return;
    }
    downloadInBrowser(json);
    setNote(m.config_downloaded());
  }

  async function importConfig(json: string) {
    setNote(null);
    const imported = await importer.mutateAsync(json);
    setNote(m.config_imported({ lists: m.count_block_lists({ count: imported }) }));
  }

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          icon={<Download />}
          disabled={exporter.isPending}
          onClick={exportConfig}
        >
          {m.common_export()}
        </Button>

        <Button
          variant="outline"
          size="sm"
          icon={<Upload />}
          disabled={importer.isPending}
          onClick={async () => {
            if (!isTauri()) {
              filePicker.current?.click();
              return;
            }
            const json = await pickConfigurationFile();
            if (json) await importConfig(json);
          }}
        >
          {m.common_import()}
        </Button>

        {/* Browser fallback for the native file dialog. */}
        <input
          ref={filePicker}
          type="file"
          accept="application/json,.json"
          className="hidden"
          onChange={async (e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) await importConfig(await file.text());
          }}
        />
      </div>

      {note && <p className="mt-2 truncate text-success text-xs">{note}</p>}
      <InlineError error={exporter.error ?? importer.error} />
    </>
  );
}

function downloadInBrowser(json: string) {
  const url = URL.createObjectURL(new Blob([json], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = "focuser-config.json";
  link.click();
  URL.revokeObjectURL(url);
}
