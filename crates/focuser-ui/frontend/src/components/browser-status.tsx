import { Check, CircleAlert, ExternalLink } from "lucide-react";
import type { BrowserStatus as Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import { QueryState } from "@/components/ui/feedback";
import { useBrowserStatus } from "@/lib/commands";
import { isTauri, openInBrowser } from "@/lib/native";
import { m } from "@/paraglide/messages.js";

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

/** Where a browser's own UI lets someone grant an extension incognito access. */
function extensionSettingsUrl(browser: Status): string {
  return browser.browser === "Firefox" ? "about:addons" : "chrome://extensions/";
}

function BrowserRow({ browser }: { browser: Status }) {
  // Connected but not covering incognito is a real gap, not a lesser one —
  // a private window in that state has nothing blocking it at all.
  const incognitoGap = browser.extension_connected && !browser.incognito_allowed;

  return (
    <li className="flex items-center justify-between gap-4 px-5 py-4 transition-colors hover:bg-hover/40">
      <div className="min-w-0">
        <p className="truncate font-medium text-foreground text-sm">{browser.display_name}</p>
        <p className="mt-0.5 flex items-center gap-1.5 text-xs">
          {browser.extension_connected ? (
            incognitoGap ? (
              <>
                <CircleAlert aria-hidden className="size-3.5 text-warning" />
                <span className="text-warning">{m.browser_incognito_not_allowed()}</span>
              </>
            ) : (
              <>
                <Check aria-hidden className="size-3.5 text-success" />
                <span className="text-success">{m.browser_extension_connected()}</span>
              </>
            )
          ) : browser.running ? (
            <>
              <CircleAlert aria-hidden className="size-3.5 text-warning" />
              <span className="text-warning">{m.browser_running_without()}</span>
            </>
          ) : (
            <span className="text-faint-foreground">{m.browser_not_running()}</span>
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
          {m.common_install()}
        </Button>
      )}
      {incognitoGap && isTauri() && (
        <Button
          variant="outline"
          size="sm"
          icon={<ExternalLink />}
          onClick={() => openInBrowser(browser.launch_name, extensionSettingsUrl(browser))}
        >
          {m.browser_allow_incognito()}
        </Button>
      )}
    </li>
  );
}
