# Failed setup: Task Scheduler XML correction — 18 October 2026

The user's setup reached SCM startup, then returned exit 25 while registering
`\Focuser\UpdateRecovery`. Windows reported invalid XML at
`LogonType:ServiceAccount`. The authority is already running from the partial
installation; a setup error does not mean that SCM provisioning was rolled back.

## Cause and changes

`TASK_LOGON_SERVICE_ACCOUNT` is an API/COM enum, not a permitted XML `LogonType`
value. The SYSTEM XML now identifies `S-1-5-18` and omits the optional element.
The independent agent retains `InteractiveToken` and `LeastPrivilege`.
The task-ownership check used by repair/removal now recognizes that SYSTEM XML.
See Microsoft's [XML schema](https://learn.microsoft.com/en-us/windows/win32/taskschd/task-scheduler-schema)
and [API enum](https://learn.microsoft.com/en-us/windows/win32/taskschd/principal-logontype).

Changed source: service `update_broker.rs`, `main.rs`, integration test
`tests/task_schema.rs`, and `scripts/test-task-schema.ps1`. Error reporting now
emits one diagnostic, with ANSI disabled. No extension, frontend, policy,
commitment-expiry, IPC authorization or update-trust code was changed.

## Existing failed installation

Rerunning the previous package repeats its XML defect. The corrected installer
also retains the exact-code repair barrier, so it cannot itself replace that
already-running, unsigned service with a different build.

`target/windows-task-schema-fix/Repair failed setup.cmd` therefore opens a
**one-off explicit administrator recovery** for this particular failed build.
This is a local operator artifact, not a shipped updater, legacy bootstrap,
installer exemption, service command or client-supplied unlock flag. It is bound
to the exact old/new executable hashes, installer hash, enrollment identity and
owner SID. Other builds or damaged maintenance states are refused.

The recovery checks the live authority's stored removal decision, requests an
ordinary SCM stop (which the authority may refuse), acquires storage locks and
preserves a private, verified backup of current authority data and the old
service executable. It never force-kills the authority or changes SCM recovery.
It atomically replaces only the known defective service executable, then opens
the corrected package's ordinary `/REPAIR` path. There is no import from the
earlier empty archive and no database reset, so commitments made since the
previous attempt remain in the current database with their original expiry.
Pre-replacement failures resume the prior running authority when possible;
backups are retained. Interrupted-update/removal journals are never cleared.

The existing pinned broker and its metadata are left unchanged. The fixed
provisioning code can reuse that protected broker while registering valid XML.
Signed update authorization remains unavailable in this unsigned build; this
recovery does not grant update authority or configure a signing bypass.

Close previous setup windows, then run the recovery launcher. It requests
administrator elevation and opens normal setup repair after its backup/code step.
The launcher has **not** been applied by the agent. Only `-InspectOnly` was run:
it passed without stopping the service, replacing files or launching setup.

## Validation

- **96 service tests passed** (all targets), including SYSTEM task ownership,
  session-agent separation and existing uninstall/persistence/pipe tests.
- Strict service Clippy and workspace formatting passed.
- On Windows, `ITaskFolder.RegisterTask` with **TASK_VALIDATE_ONLY = 1** accepted
  both actual generated XML fixtures and rejected the old `ServiceAccount` XML
  and invalid agent privilege XML. Before/after checks confirmed that no tasks
  were registered; no task was run.
- An unelevated helper request was rejected before provisioning and emitted one
  plain `elevation_required` error, without ANSI escapes.
- Optimized GNU release/NSIS build passed. Actual rendered-installer authorization
  audit, 13 unsafe-script mutations and scratch ACL checks passed.
- One-off recovery parsing and read-only live preflight passed. The original
  installed service hash and Running state were verified unchanged afterward.
- Scratch testing of the actual static recovery file-helper module passed:
  nested file backups, private ownership/ACLs with storage locks held, original
  executable backups, and atomic replacement retaining effective access rights.
  The recovery uses an explicit atomic-backup filename for Windows PowerShell's
  `File.Replace` binding and reapplies the original code permissions.
- The actual administrator replacement/backup/repair workflow, task registration,
  installed uninstall, logon and reboot remain **unverified** until the user runs
  direct acceptance. No VM or product installer was run during this fix.

## Artifacts

Directory: `D:\Focuser\target\windows-task-schema-fix`.

- `Repair failed setup.cmd` / `Launch-Repair.ps1` / `Repair failed setup.ps1`
- `RepairFileOperations.psm1` and the scratch-only `Test-Repair-FileOps.ps1`
- `Focuser_0.10.0_x64-task-schema-fix-setup.exe` — unsigned optimized candidate,
  25,856,581 bytes; SHA-256
  `F1AC1818610F701DC9C1B79634D7E8D2D9F5F9B8446C5A53D34C4F1ED11EDE8A`
- `focuser-service-fixed.exe` — SHA-256
  `1350DB49BD5809207A4CF40F8694DF652E359B2E57F76FCB74C91DC1BEFB812D`
- Adjacent test/build/Clippy logs, validation reports and rendered installer.

The previous packages and protected data archive were preserved. No further
phase, signing, publishing or cloud feature is implemented by this correction.
