# Windows setup, repair and release candidate

This work covers setup/repair, release packaging, and enforcement activation.
It does not implement another update phase or change the browser extension.

## Setup and repair

Setup detects the interactive desktop user's SID from the desktop shell's real
Windows token, rather than assuming the elevated installer account owns the
policy. Normal setup no longer requires typing a SID. Advanced enrollment can
still supply `/OWNER_SID=`. If discovery fails (for example, there is no desktop
shell), setup explains how to choose the intended user explicitly. Silent fresh
setup still requires an explicit `/EMPTY` or `/IMPORT=...` data choice.

When setup finds existing files or registration, interactive setup offers Repair.
Silent repair requires `/S /REPAIR`. The private bundled helper:

- Repairs an enrolled installation only when the installed service executable
  exactly matches the bundled helper. It verifies protected paths and preserves
  existing configuration, SQLite data, commitments and binary files. The existing
  provisioning path repairs the service/task/native registration without stopping
  enforcement or treating repair as an update.
- Can archive an incomplete pre-provisioning payload and continue fresh setup
  only after checking authority storage, SCM, startup tasks, uninstall and native
  registrations. Unknown files, policy data and reparse points prevent archival.
  Archived files are retained beside the installation directory; nothing is
  deleted to make setup succeed.
- Refuses mismatched builds, missing authoritative data and interrupted
  maintenance with a recovery diagnostic. It cannot bootstrap an older build or
  clear a commitment. Keep the exact installer for same-code repair.

Registration/shortcut/uninstaller creation is shared by fresh setup and successful
same-code repair. A failed provision tells the user to retry this installer with
Repair and retains the original error in Details. WebView2 remains a checked
prerequisite; its absence fails before provisioning.

### Missing installed executable during Repair

`repair_payload_missing` means protected authority storage exists but the installed
service executable is absent. It is not a relative-path setting and cannot be fixed
by entering another SID. Repair now identifies the exact missing path; other file
validation failures include their path and the Windows filesystem diagnostic.

Use the explicit **Interrupted maintenance and administrator recovery** procedure in
`service-phase2.md`. An administrator must preserve the complete private data
directory and restore the original matching installation files from a trusted copy.
Do not delete `C:\ProgramData\FocuserAuthority`, choose `/EMPTY`, move a random
newer service into place, or use the pre-provisioning recovery script for this
condition. The archived `Focuser-Incomplete-*` payload is not automatically assumed
to match the enrolled build. After restoring that build, use its original installer
for same-code repair. Ordinary Repair cannot replace an unknown or missing enrolled
build with this candidate.

For the particular failed setup on this development machine, a later elevated
read-only check proved that the retained database had no policy, commitments or
history. The explicit administrator recovery utility
`scripts/recover-empty-abandoned-setup.ps1` archived every original file with its
private ACL intact. It is not shipped or called by ordinary install/uninstall and
refuses any retained policy/history or mismatched resources. The user can run
`target/windows-repair-diagnostic/Continue Setup.cmd` to open the unchanged
installer with the original SID and protected database import prefilled. Recovery
did not launch setup. Details and checksums are in `service-direct-test.md` and
the adjacent `recovery-receipt.json`.

## Enforcement and release

The independent service and agent may remain idle between blocking periods.
Keeping them available does not itself authorize blocking or prevent uninstall.
Website recovery enforcement now requires an effectively active list with an
enabled website rule, or an enabled website quota. Empty lists, app-only lists,
disabled website rules and inactive schedule periods do not activate it.
Executable/path/title rules retain their existing active-list evaluation.

Before the agent can apply browser restrictions, the service records cleanup
responsibility and checks that browser blocking is still active. Stale snapshots
cannot start new browser enforcement after release. After coverage ends, the
agent restores owned settings and confirms completion through authenticated IPC.
That clean state persists across service restart, so an idle installation with
no outstanding browser cleanup does not require a live agent merely to uninstall.
Unknown state from older installations and pending/failed restoration still
require an agent or the explicit administrator recovery procedure.

Uninstall commitments remain authoritative and span temporary pauses/Pomodoro
breaks. Removal is allowed once every relevant commitment expires or is verified-
unlocked. Expiry does not extend a commitment or create another one. Enabled
untimed lists and enabled quotas continue to run until disabled according to
their existing rules; a protection timer does not silently delete those rules.
Service unavailability continues to fail ordinary removal with a clear error.
With no hosts fallback block, the service leaves unrelated hosts-file bytes
unchanged. Ending a fallback block removes only Focuser's marked section and
preserves other entries and their line endings.

## Release packaging

`scripts/build-windows-installer.ps1` builds the existing per-machine NSIS format
only. Release-mode output requires code signing by default. Configure:

- `FOCUSER_SIGN_CERT_SHA1`: certificate-store thumbprint, with private-key access
  already configured locally or on the release runner.
- `FOCUSER_AUTHENTICODE_SHA256`: SHA-256 fingerprint of that certificate, used
  as the service's publisher pin.
- `FOCUSER_SIGN_TIMESTAMP`: HTTPS timestamp-server URL.

`scripts/sign-windows-file.ps1` requires a valid code-signing certificate,
matching publisher pin, valid signature and timestamp. Sidecars are signed before
packaging; Tauri's signing command signs the UI, generated uninstaller and setup.
Do not put private keys/passwords in command lines, source or chat. Successful
signing has not been tested here because no certificate is available.

For an explicit local **unsigned release candidate**, use:

```powershell
./scripts/build-windows-installer.ps1 -Target x86_64-pc-windows-gnu -HostBuild -AllowUnsignedRelease
```

`-HostBuild` requires the matching Rust host toolchain. Public release automation
does not pass the unsigned override. The build writes a manifest recording
profile, target, signature status, artifact sizes and SHA-256 hashes. An unsigned
candidate is not a signed public release and does not configure trusted updates.

## Acceptance boundary

Automated checks use scratch data/development pipes and do not install the
product. Neither the installer nor persistent enforcement is activated by this
work. Actual UAC alternate-account discovery, installed same-code repair, SCM
startup/reboot, signature acceptance and installed interactive/direct/silent
uninstall still require deliberate Windows acceptance. Existing recovery remains
in [service-phase2.md](service-phase2.md#interrupted-maintenance-and-administrator-recovery).
