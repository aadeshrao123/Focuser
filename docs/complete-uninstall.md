# Complete removal

The new Start menu entry **Focuser → Uninstall Focuser Completely** starts the
installed uninstaller with `/DELETE_DATA`. The repository's
`Uninstall Focuser Completely.cmd` is a launcher for that same operation. It
does not delete anything itself and refuses installed helpers predating this
feature. Close the Focuser interface first and remain signed in as the enrolled
user, with their unprivileged session agent connected.

If the entire installed directory is already absent, the repository launcher now
uses the explicit [leftovers recovery tool](leftovers-recovery.md). It verifies a
cold runtime and stored commitments before archiving data/removing orphaned
registrations. An existing installation's unavailable service is still an error.

The launcher checks that the installation directory and uninstall registration
are gone before reporting completion. This also guards against interpreting the
initial launcher's exit code as the result of NSIS's
[temporary child uninstaller](https://nsis.sourceforge.io/When_I_use_ExecWait_uninstaller.exe_it_doesn%27t_wait_for_the_uninstaller).

Interactive, direct `uninstall.exe /DELETE_DATA`, and silent
`uninstall.exe /S /DELETE_DATA` use the same authorization barrier. Complete
removal refuses while any list with enabled blocking rules is running, an active
Pomodoro session remains, or a stored commitment remains. Allowance pauses and
Pomodoro breaks do not end stored
commitments. A schedule gap with no overlapping commitment is eligible. After
expiry or verified unlock, an independently enabled always-on block must also
be ended/disabled in Focuser. Denials name the lists and their release conditions.
The retained-data option preserves legacy uninstall opt-outs; the complete mode
explicitly requires all running blocks to end.

The service refreshes its stored policy at preparation and consumption under
the existing policy/maintenance lock. Its single-use permit remains bound to
the actual Windows caller, installation, policy revision and data choice. A new
complete-removal operation is deliberately rejected by older authorities; no
fallback to their weaker data-deletion operation is attempted. A separate agent
acknowledgement prevents an older agent from claiming it removed user data.
Neither a checkbox nor `/DELETE_DATA` grants authority.

After complete-removal consumption, UI/native clients receive an authenticated
cooperative exit request so WebView releases its profile. The service and agent
remain alive for cleanup. A merely allowed decision or prepared permit cannot
request that exit. The agent restores only journaled browser settings, removes
matching user startup/native-host entries and matching user shortcuts, then
deletes these exact app-owned directories in **its own unprivileged session**:

- Roaming and Local AppData: `focuser\Focuser`
- Roaming and Local AppData: `com.focuser.app` (including Tauri/WebView caches)

All live observed enrolled sessions must acknowledge. An absent agent, old agent,
locked WebView profile, failed restoration or timeout prevents helper cleanup.
If an already-authorized attempt aborts after user cleanup, some preferences or
legacy profile data may already have been deleted; the authoritative policy
still remains until the later service cleanup. There is no rollback promise for
explicitly requested data erasure.

The helper removes matching machine native-host registrations, waits for clients
to exit, stops SCM normally and waits for its actual process exit, removes the verified session and update-recovery
tasks, removes their task folder only if empty, and deletes service registration.
Complete removal then deletes the private `ProgramData\FocuserAuthority` tree,
including SQLite sidecars, service configuration, update broker/staging/cache,
operation logs, lock files and removal tombstones. Retained-data removal keeps
that private tree for explicit recovery/import. NSIS deletes the enumerated
payload, manifests, its shortcuts and uninstaller, verifies the installation
directory was removed, and only then deletes uninstall registration.

Tree cleanup inventories every entry before deleting anything, bounds depth and
entry count, rejects reparse points and redirected ancestors, and pins directory
handles while traversing children. Elevated cleanup additionally verifies private
ACLs on every service-storage entry. Unknown files in the installation directory
are **not** recursively swept: they cause exit 27 with a partial-cleanup message.
Locked objects and permission failures are errors, never delayed reboot deletion
or a successful result. A failure after SCM removal requires explicit recovery;
retrying cannot manufacture fresh authorization from missing policy.

Scope intentionally excludes browser profiles/extensions, other applications,
the source checkout, installer downloads and separately preserved recovery
archives/backups. Windows-maintained event/diagnostic history is not erased.
Only the currently enrolled user's app data is covered; data left in other users'
profiles by historical per-user versions requires separate owner review.

If the authority cannot be contacted, ordinary removal fails. If files were
already deleted by another uninstaller, use the explicit administrator recovery
procedure in [service-phase2.md](service-phase2.md). This launcher cannot bypass
that condition or turn retained commitments into an empty policy. Development
does not install or activate this feature on the workstation.

## Acceptance checks requiring a real installation (not performed here)

1. Back up the existing authority before any live test. Install this build on a
   deliberately selected test installation; do not apply it over damaged retained
   enrollment or claim the installer is an authenticated update.
2. With overlapping timed commitments, try Start menu, direct and silent complete
   removal. Expect exit 26, names/releases in Details, and unchanged SCM, tasks,
   registrations, payload and database. Repeat during a pause/Pomodoro break.
3. Close/terminate only Tauri, restart only the authority through the supported
   test controls, and verify original expiry/usage and removal denials persist.
4. End all blocks by expiry or verified unlock plus ending any enabled list.
   Close Tauri, keep the agent connected, then complete removal. Verify no owned
   SCM entry/tasks/native hosts/marked hosts section, installed payload, shortcuts,
   private authority tree or listed enrolled-user data roots remain. Verify browser
   settings were restored and unrelated files/registrations/backups survived.
5. Repeat the allowed case silently; verify exit 0 only after cleanup. Repeat with
   a missing service, missing/old agent, locked WebView file and unexpected payload
   file. Require bounded failure, no false success, and diagnostics/recovery for
   any partially completed removal.
6. Reboot after successful removal and verify no Focuser process/task/service
   restarts. Fresh explicit enrollment should not encounter old authority
   tombstones. Repeat retained-data removal and confirm history/enrollment remain.

Automated policy, protocol, permit, ordering, restart and scratch-filesystem tests
are separate from these unverified SCM/Task Scheduler/WebView/end-to-end checks.
