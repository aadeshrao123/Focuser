# Windows provisioning and authorized removal (phase 2)

Implementation is for one enrolled Windows SID and one concurrent interactive
session. The service remains LocalSystem; the independently scheduled agent uses
InteractiveToken and LeastPrivilege. UI autostart is optional and unrelated to
agent startup. No installer, SCM service, logon task, hosts enforcement or browser
policy was activated during initial build validation. The user subsequently chose
direct testing without a VM; see `service-direct-test.md` for the installer defect,
scoped recovery and current acceptance procedure.

## Supported package and lifecycle

Only the per-machine **NSIS EXE** built by `scripts/build-windows-installer.ps1`
is the Windows release package. `tauri.windows.conf.json` selects NSIS, all four
sidecars, and no updater artifacts; release CI no longer publishes a Windows
portable executable, MSI or updater ZIP. Tauri Rust is pinned at 2.10.3/build
2.5.6; the bundler CLI is pinned at **2.10.1**. Other platforms retain their
existing packaging configuration.

The full `nsis/installer.nsi` template replaces the stock template. Inspection of
upstream tauri-cli-v2.10.1 `installer.nsi` found a reinstall page invoking an old
uninstaller, `/UPDATE` paths, a WebView2 section executing before PREINSTALL,
`CheckIfAppIsRunning`, and independent file/registry/data cleanup. Its
PREUNINSTALL hook is before its process check, but hooks alone do not protect the
whole install/upgrade lifecycle. Our template has no stock reinstall, WebView
bootstrap/download, MSI migration, process-killing or updater path. Existing
installation/service/ProgramData state is rejected in `.onInit` and checked again
before copying files. **WebView2 must already be installed** (checked before files or enforcement are provisioned) (normally present on
supported Windows 10/11 images); installer bootstrap support is deferred.

A failed provision is reported as incomplete, with existing policy/data retained.
Repair uses the already installed protected helper, not a new installer that
replaces running binaries. Fresh provisioning and repair use the same configuration
comparison; changing owner SID or installed paths is rejected. Repair can repeat
SCM configuration and task registration without replacing the database. A private
maintenance lock serializes elevated repair/removal helpers, and repair refuses
missing storage or an active/interrupted removal journal instead of creating an
unrestricted replacement. A damaged,
partial configuration or interrupted removal requires explicit recovery.

Explicit fresh silent examples (get the SID with `whoami /user` in the owner's
unelevated session):

```powershell
# Choose exactly one: explicit profile migration or explicit empty enrollment.
.\Focuser-setup.exe /S /OWNER_SID=S-1-5-21-... /IMPORT="C:\Users\Owner\AppData\Roaming\focuser\Focuser\data\focuser.db"
.\Focuser-setup.exe /S /OWNER_SID=S-1-5-21-... /EMPTY
# Repair the SAME installed files, not a binary upgrade:
& 'C:\Program Files\Focuser\focuser-service.exe' install --repair --owner-sid S-1-5-21-...
```

The GUI collects the SID and migration choice explicitly. Credential elevation
must enroll the intended interactive user's SID, not the administrator supplying
UAC credentials. There is no automatic import and no implicit new empty database.
The online backup migration checks all tables and preserves commitments, budgets,
history and the source, including WAL contents. Installer files inherit a protected
Program Files directory (Administrators/SYSTEM full, Users read/execute), with its
owner set to Administrators. The service verifies files and ancestors. Authoritative
ProgramData files retain the private SYSTEM/Administrators ACL from phase 1.

SCM is automatic, LocalSystem, and has restart delays 5/10/30 seconds followed by
NONE, reset after 24 hours. Non-crash errors also count. Standard users can query,
not change/stop the service. The task `\Focuser\SessionAgent` logs on only the owner,
uses IgnoreNew plus a per-session user-specific named mutex, restarts on failure
up to three times, and never launches Tauri or requests elevation. Registration
is valid even if immediate task execution must wait for the enrolled user's next
logon; a failed immediate start is explicitly logged as `agent_pending_logon`.
UAC-disabled/built-in Administrator sessions need an enrolled standard account;
the agent refuses an elevated token. Reconnecting
retains the event cursor; a new service generation skips obsolete feedback.
Logoff leaves enforcement with the service; absence is an explicit degraded state.
Multiple simultaneous interactive sessions remain unsupported by the single
localhost browser endpoint; this is not multi-user enrollment.

## Removal semantics and authority

`RemovalDecision` returns `removal_allowed` or `removal_protected`, every blocking
list's ID/name, `manual_commitment` or `scheduled_occurrence`, and release times.
A null release on a blocking commitment means an authorized unlock is needed.
The aggregate release is the latest current blocker, not a promise about future
schedule occurrences. A fresh decision is required before removal.

Uninstall commitment semantics (basic completion fix):

* Manual `prevent_uninstall` commitments span schedule gaps, temporary disabled
  blocking states and pauses until their original expiry or verified unlock. A scheduled lock covers its locked occurrence only;
  a gap or previously authorized occurrence unlock is not made stricter.
* Pomodoro pauses and short/long breaks do not release an existing uninstall
  commitment. Blocking may still disable normally during a break; removal checks
  the stored manual commitment independently. No expiry is extended or restarted.
* Every overlapping list is evaluated, not just the selected list. Removal does
  not infer uninstall protection from the separate service-stop flag when the
  user explicitly left uninstall protection off. Ordinary stop and full removal
  remain separate policy operations.

The local pipe grants limited connection rights to the owner and elevated
administrators, rejects remote clients, validates the actual impersonation token
and process metadata, and retains authenticated SCM/SYSTEM endpoint validation.
Non-owner administrators cannot use ordinary policy commands. Maintenance requests
additionally require the protected installed service executable and an actual
elevated Administrators token. Neither an identity string, UI challenge result,
`/UPDATE`, unlocked boolean, nor an updater claim grants authority.

The helper obtains a random 15-second, single-use removal permit, scoped to the
running installation instance, operation, caller SID/PID/session/image, retention
choice and policy revision. Under the same mutex as all commands, browser writes,
agent observations and ticks, consumption checks revision and re-evaluates current
wall-clock protection. Commands invalidate pending permits conservatively, even
read commands; an intervening UI poll may require retrying removal. A consumed
permit cannot be replayed. Restarting the authority invalidates outstanding
permits. No general upgrade permit exists: `upgrade_deferred` is returned before
binary replacement. All binary upgrades, including same-version reinstalls, are
currently deferred; retained data/commitments are never overwritten to emulate
an upgrade.

After consumption, writes/transitions freeze and a durable private SQLite
maintenance journal is set. The agent suspends its guard under a lock, restores
only journaled browser settings, and acknowledges the transaction. Removal requires
an enrolled active agent and acknowledgements from all still-live observed owner
sessions; absence, timeout or failed restoration prevents cleanup. Restoration of
registry policies may require the user to restart a browser to refresh its cached
policy; the installer does not kill browsers.

The service then clears only its marked hosts section and permits a normal SCM
stop. Expected stop does not invoke failure recovery. The service requests a clean, zero-code agent exit while IPC remains available;
the helper waits for that exit before stopping SCM. It then ends/deletes only a
task whose parsed XML has the enrolled principal, interactive least-privilege
logon, installation marker and single expected executable action. It checks for
late agent instances and deletes only matching machine native-host registrations, deletes its SCM
  registration. The current complete-removal mode erases the entire private
  authority tree and enrolled-user app data after authorization; see
  [complete-uninstall.md](complete-uninstall.md) for scope and its stronger
  no-running-block requirement. NSIS
then deletes only enumerated installed binaries/manifests and its shortcut. A
locked file or any required cleanup failure returns failure, never a reboot-pending
success. Keep the UI closed before uninstall. External profile/import originals,
other applications' browser policies and unrelated hosts lines are preserved.

```powershell
# Same authorization barrier in Settings, direct and silent invocation.
& 'C:\Program Files\Focuser\uninstall.exe' /S               # retain authoritative data
& 'C:\Program Files\Focuser\uninstall.exe' /S /DELETE_DATA  # delete authoritative data
```

Missing authority fails within bounded transport timeouts. There is no fallback
writable database. NSIS exit 22 means upgrade/reinstall deferred, 24 invalid silent
enrollment, 25 incomplete provisioning, 26 removal denied/incomplete before installer
file cleanup, 27 partial file cleanup, 28 missing WebView2 prerequisite. Helper diagnostics include stable protocol
reason codes. NSIS's silent exit code is intentionally coarser; details are in the
installer log/operation logs. Code 0 alone is not VM acceptance evidence.

## Interrupted maintenance and administrator recovery

For an already missing Program Files installation, an automated, checked cold
recovery route is now available: [leftovers-recovery.md](leftovers-recovery.md).
The manual steps below remain for states that route cannot verify.

A helper crash/stall before SCM stop resumes service enforcement (within 120 seconds,
normally the next tick when its PID is gone), keeps the journal and freezes client
mutations. Service restart also resumes enforcement with existing commitments and
reports interrupted maintenance. No replacement engine or empty database is started.
After SCM has stopped, a later helper failure leaves the persistent removal journal;
reboot starts the still-registered authority again unless registration was already
removed. This is a partial removal, not successful uninstall. Immutable step records
and `removal-progress.json` prevent an installer from silently reinstalling over it.
Retained-data removal keeps the private installation tombstone/config/logs;
re-enrollment then requires an explicit administrator recovery/archive step.
Successful complete removal now deletes that tree as documented in
[complete-uninstall.md](complete-uninstall.md); older builds retained it even
after deleting the database.

Ordinary removal never uses recovery as an unavailable-service fallback. A determined
administrator can replace files/change SCM; this is not administrator-proof protection.
The following is an explicit, manual recovery procedure for a damaged VM installation:

1. Record the failed operation, SCM/task state, installed image paths and journal
   records. Preserve a VM snapshot. Temporarily disable only the Focuser agent task
   with Task Scheduler, and change only `FocuserAuthority` startup to **Manual**.
   Reboot normally so Windows shuts the service down. Do not disable Windows recovery
   or change unrelated services. Verify the Focuser service and agent are stopped.
2. As administrator, copy the entire private ProgramData directory (including WAL/SHM)
   and installed binaries to an ACL-protected recovery location. Keep the originals
   until the restored authority has been validated. Do not copy only a live SQLite DB.
3. For **resume enforcement**, restore missing binaries from the same trusted build
   and preserve the enrolled configuration and DB. With the service still stopped,
   use a SQLite transaction to set ONLY the existing settings key
   `_authority.maintenance` to an empty string. Back up SQLite with its backup API
   first; do not reset lists, usage, installation identity or other settings. Archive
   removal-progress/step files alongside the backup rather than treating their
   absence as ordinary removal authorization. Restore Automatic SCM startup and
   enable the owner task, then run the installed helper's `install --repair` with
   the same SID. Verify commitments and usage before resuming use.
4. For **explicit emergency removal**, after that backup, manually remove only the
   recorded Focuser SCM entry, verified Focuser task, matching native-host registry
   values, installed Focuser files and marked hosts section. Restore the owner's
   browser recovery journals in that user's session using a trusted agent build;
   never delete whole Chrome/Firefox policy keys. Choose retention/deletion explicitly.
   If the journals cannot be restored, report the remaining restrictions and keep
   their backup; do not claim successful cleanup.
5. A retained-data reinstall is a recovery/import decision, not an update. Archive the
   old private directory with its ACL intact, then explicitly import the recovered DB
   during fresh enrollment. Clear its old maintenance marker only after verifying the completed-removal records and preserving the original backup as in step 3. Never silently choose `/EMPTY` over retained commitments.

These steps are administrator actions outside the ordinary installer protocol. No
client `--force`, maintenance bypass flag or hidden updater exception implements them.

## Validation and next phase

See `service-phase2-validation.md` for executed checks and `service-vm-test.md` for
OS acceptance. Build/inspect an installer without executing it. Run
`scripts/audit-windows-installer.ps1` against the actual rendered installer, not just
the template; release CI runs this check and refuses MSI/updater ZIP output. Inspect
compiler warnings and the rendered `.nsi` before releasing. The GNU debug package
also carries its required `WebView2Loader.dll`; resource extraction and scoped
cleanup are audited in the actual rendering. Production signing and
SCM/ACL/task/UAC/reboot acceptance remain required in a disposable VM.

Next phase: signed, service-coordinated updates with validated package identity,
protected staging, rollback and recovery across power loss, preserving installation
identity, policy and usage. Do not enable Tauri's updater until that protocol exists.
