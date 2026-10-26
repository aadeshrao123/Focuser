# Browser updates and recovery

This change is confined to the unprivileged session agent. Browser extension
source, stored commitments, service authorization and maintenance permits are
unchanged. It does not introduce an updater-claim bypass or exempt arbitrary
processes named `setup.exe` from configured application rules.

Two implementation gaps were found while investigating the reported Brave
lockout after an automatic update:

* Installation recovery's deny-all URL policy had no browser-update exceptions.
  It now permits the browser's specific About/update page and dedicated update
  delivery endpoints. The extension store exceptions and deny-all policy remain.
  General settings, arbitrary downloads and broad vendor-domain exceptions are
  not added. Firefox's native About dialog is not a URL navigation; its update
  service/download paths are included in the WebsiteFilter exceptions.
* A browser could update and relaunch between guard polls. The previous process's
  profiles then survived indefinitely in the watched-profile union. The guard
  now re-discovers profiles when all main-process IDs change. Existing recovery
  journals and the authority's grace deadline survive; a new process is not an
  unlock or proof of extension coverage. Fresh covered reconnection still exits
  recovery without killing/restarting the healthy browser.

Old recovery journals are extended before policy writes. Their original saved
values and types remain the restoration source; the old recovery policy is never
recaptured as the original. An unexpected journal shape or external policy change
is rejected. Tests cover interruption after extending the journal and restoration
after successfully applying the expanded policy, using unique scratch registry
keys rather than installed browser policy keys.

Background updater executables are not browser main processes and remain outside
the extension guard. Their presence in Task Manager alone does not establish that
Focuser terminated them. No process termination or browser update was triggered
on the development machine during validation.

Endpoint references:

* [Brave's documented network requests](https://github.com/brave/brave-browser/wiki/Network-requests-in-Brave)
* [Brave's release/update checklist](https://github.com/brave/brave-browser/wiki/Desktop-Release-Checklist)
* [Google's update endpoint guidance](https://support.google.com/chrome/a/answer/6350036?hl=en)
* [Microsoft's Edge endpoint guidance](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-security-endpoints)
* [Mozilla's update process](https://firefox-source-docs.mozilla.org/toolkit/mozapps/update/docs/InAppUpdateProcess.html)

Executed: 30 session-agent tests passed, including five additional tests for
update/relaunch behavior and recovery-policy compatibility. All tests use test
data or isolated scratch registry keys. Formatting and diff whitespace checks pass.
Installer build/audit details are recorded with the generated artifact in
`C:/Users/Ahmed Ameen/AppData/Local/FocuserDevTools/browser-update-fix-20261020`.

The rebuilt unsigned NSIS installer was compiled from the audited rendered
template with only the session-agent payload replaced. Its authorization/order
audit, 13 negative installer fixtures, and scratch file/nested-resource ACL checks
passed. The existing service, CLI, native host and UI build were preserved.
`D:/Focuser/Focuser-Setup-CURRENT.exe` has SHA-256
`F2F4CDF19F24B243304F920F6581A737AEA74057DDEC88C48916E0449EA8DA17`.
The agent SHA-256 is
`211E1358FF483D7401E5D88B87456538241A45D7E6A2D770D3044EE175CF6ED3`.
The previous CURRENT installer and previous agent were archived in that build
directory. No setup was executed. At inspection the SCM service and installed
agent were absent. If retained ProgramData still exists, the missing-payload
administrator recovery remains necessary before ordinary repair; this browser
fix does not bypass retained-enrollment checks or initialize an empty policy.

Remaining acceptance: reproduce a real Brave update with an enabled, covered
extension and an active web block. Confirm the updater completes, the new browser
instance reconnects, blocked sites remain blocked, and normal browsing does not
require uninstalling Focuser. Repeat with missing extension coverage: only recovery
and update access should be available, and neither updates nor pauses should
release the commitment. Exercise an existing recovery journal, then let the block
expire and verify the original browser policies are restored. Check externally
managed policies still report conflict instead of being overwritten. Those live
Windows/browser behaviors have not been verified by the automated tests.

This remains an unsigned development build. It contains no experimental kernel
driver and does not resolve the previously documented administrator-tampering or
signed-update deployment limitations. No persistent enforcement was activated.
