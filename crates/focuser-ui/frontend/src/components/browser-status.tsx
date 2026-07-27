import { Check, CircleAlert, ExternalLink } from "lucide-react";
import type { BrowserStatus as Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import { QueryState } from "@/components/ui/feedback";
import { useBrowserStatus } from "@/lib/commands";
import { isTauri, openInBrowser } from "@/lib/native";

export function BrowserStatusList() {
  const browsers = useBrowserStatus();

  return (
    <QueryState
      isPending={browsers.isPending}
      error={browsers.error}
      onRetry={() => browsers.refetch()}
      isRetrying={browsers.isFetching}
    >
      <ul className="divide-y divide-border">
        {browsers.data?.map((browser) => (
          <BrowserRow key={browser.browser} browser={browser} />
        ))}
      </ul>
    </QueryState>
  );
}

function BrowserRow({ browser }: { browser: Status }) {
  return (
    <li className="flex items-center justify-between gap-4 px-4 py-3">
      <div className="min-w-0">
        <p className="truncate font-medium text-foreground text-sm">{browser.display_name}</p>
        <p className="mt-0.5 flex items-center gap-1.5 text-xs">
          {browser.extension_connected ? (
            <>
              <Check aria-hidden className="size-3.5 text-success" />
              <span className="text-success">Extension connected</span>
            </>
          ) : browser.running ? (
            <>
              <CircleAlert aria-hidden className="size-3.5 text-warning" />
              <span className="text-warning">Running without the extension</span>
            </>
          ) : (
            <span className="text-faint-foreground">Not running</span>
          )}
        </p>
      </div>

      {!browser.extension_connected && isTauri() && (
        <Button
          variant="outline"
          size="sm"
          icon={<ExternalLink />}
          onClick={() => openInBrowser(browser.launch_name, browser.store_url)}
        >
          Install
        </Button>
      )}
    </li>
  );
}
