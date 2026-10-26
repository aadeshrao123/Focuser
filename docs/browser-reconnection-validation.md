# Browser reconnection correction — 2026-10-19

The installed Brave extension had complete manifest/path/location metadata but
omitted legacy `state` and `disable_reasons` fields. A previously confirmed
removal therefore remained latched. `extension_watch.rs` now recognizes that
format while retaining explicit disabled/missing detection and requiring the
existing live check-in in the guard. `browser_guard.rs` restores recovery
policies and cancels pending retries/deadlines without restarting a successfully
covered browser.

Validation:

- All 8 focused extension-monitor tests passed.
- All 25 optimized Windows session-agent tests passed, including reconnection
  after an expired installation attempt and failed policy restoration/retry.
- Targeted rustfmt checks passed. Browser extension source was not changed.
- Optimized agent build and NSIS packaging succeeded. The previously validated
  desktop/frontend/service payloads were retained.
- Generated installer authorization/order audit, 13 negative audit fixtures,
  and rendered file/directory permission checks passed.
- Normal installed silent uninstall returned 0 after service authorization;
  service registration, application files and uninstall registration were
  verified removed. Data retention was selected.
- A private online database backup and full original authority archive were
  retained. Import preparation compared all table fingerprints, excluding only
  the verified completed removal marker in the import copy. Original data was
  not reset. Fresh setup with explicit import returned 0.
- Installed agent hash matches the corrected build. Service is running and the
  independent agent is running from Program Files. The UI was reopened.
- Original list ID, enabled state, 320 website rules and both original
  protection timestamps matched before/after. The local browser bridge returned
  606 compiled blocked-domain entries from the retained policy.
- Brave reported running, connected and private-window coverage enabled. Its
  exact main process survived a 78-second observation after initial check-in,
  exceeding the 60-second installation deadline. No new blocking commitment was
  activated and the actual extension was neither removed nor modified.

Current unsigned package: `D:\Focuser\Focuser-Setup-CURRENT.exe`

Installer SHA256:
`E67797C3E3BB3C25B30F284DAF50EBCA11F0E3F47093A9F4A89D0702EAEE7DAD`

Installed agent SHA256:
`FA15C359780CBE10B5E3A407A20CE3CA74782A34B8A6813EF69A258C4677F182`

Private retained recovery data:
`C:\ProgramData\Focuser.ReinstallRecovery-0aecc14d160b473c9df00bafec239dd7`

Operator scripts, hashes and before/after observations:
`C:\Users\Ahmed Ameen\AppData\Local\FocuserDevTools\browser-reconnect-fix-20261019`

The one-off installer wrapper's final status assignment failed because its
`error` property was absent after a successful uninstall. Separate read-only
post-install checks verified the successful installation and finalized the
operator record. No repeat install or policy mutation was used to fix that
bookkeeping issue.

Not exercised live: removing/reinstalling the actual extension during an active
protected commitment, failed policy recovery with that extension absent, other
browser families and reboot. Those transitions have targeted automated
coverage; this observation does not establish those OS acceptance cases.
No VM was used and no further protection phase was implemented.
