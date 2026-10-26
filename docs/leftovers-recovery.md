# Recovery after the installed files have already been removed

Run `Remove Focuser Leftovers.cmd` normally, then approve the Windows administrator
prompt. `Uninstall Focuser Completely.cmd` also selects this route if the entire
installed program directory is absent. A present but unavailable installation
still uses the ordinary uninstaller and fails if its authority cannot be reached.
This is an explicit administrator recovery operation, not a service-unavailable
permission fallback. It never installs, repairs or starts enforcement.

The recovery tools are built under `target/windows-leftovers-recovery/`. Their
hash manifest detects mismatched development binaries. This unsigned development
tool is run from the trusted workspace; it is not a signed production recovery
distribution. Run `scripts/Recover-FocuserLeftovers.ps1 -InspectOnly` from 64-bit
Windows PowerShell for an elevated read-only preflight without removing resources.

Machine recovery requires the Program Files installation to be absent, any
remaining authority service to be stopped and match the exact original service
path/account, and no installed Focuser processes to remain alive. It does not
terminate processes or stop a running service. Private storage/configuration,
the enrolled administrator user and maintenance/lifetime locks are checked.
The Rust helper opens the original database read-only, checks integrity/schema,
loads every list without skipping corrupt rows and uses existing commitment
evaluation. Active manual/scheduled commitments and ongoing Pomodoro sessions,
including pauses/breaks, refuse cleanup. Expired commitments and dormant rule
configuration in an absent runtime may be archived. No stored expiry, flags,
history or budget is rewritten. SQLite sidecars, corrupt/unknown schemas or
unverifiable ownership require manual recovery instead of assuming permission.

Before cleanup, the tool records the service, tasks, registry values and original
hosts bytes under a new `ProgramData\FocuserRecovery-<id>` directory. The original
authority tree is moved intact into its `Authority` subdirectory, preserving its
ACLs, after a final fresh policy/runtime check. This removes the retained
`ProgramData\FocuserAuthority` enrollment path that otherwise prevents fresh setup,
while retaining a recoverable copy. Historical backups and this source checkout
are deliberately retained. The enrolled user can read recovery diagnostics; the
original private authority data retains its private permissions.

Only matching `FocuserAuthority` registration, installation-bound SessionAgent and
UpdateRecovery tasks, exact native-host/uninstall registry entries, verified
Start menu shortcuts and the marked Focuser hosts section are removed. Task XML
checks use the principal's user ID separately from the logon trigger's user ID.
An omitted default agent RunLevel is accepted, matching
[Windows' least-privilege task setting](https://learn.microsoft.com/en-us/windows/win32/taskschd/principal-runlevel);
an explicitly elevated agent principal remains rejected. Foreign registrations,
unexpected task actions and reparse points are never swept by name.

After machine recovery, a dedicated one-shot agent mode restores user browser
settings and deletes the enrolled user's scoped Focuser data/cache and matching
user startup/native/shortcut entries without elevation. It requires installed
files, authority storage and SCM registration to be absent and rejects another
running session agent. It does not start the normal observer or browser guard.
If journals are missing, every browser-policy table Focuser uses must actually
be empty/absent; unknown policy values are preserved and require review. With
journals present, only recorded Focuser changes are restored. Browser profiles,
extensions and other applications' policy/data are preserved.

A failure is reported as incomplete, with originals/diagnostics preserved. Inspect
the printed backup location; do not treat an intermediate machine cleanup as a
successful browser/user cleanup. Running tasks, locked files, missing enrollment
alongside surviving service/tasks, alternate-account recovery, pending SQLite
journals and unknown browser policies may need the manual administrator procedure
in [service-phase2.md](service-phase2.md). No flags bypass active commitments.

Validation includes Rust read-only/active/expired/corrupt recovery fixtures,
session-agent browser/startup tests, PowerShell cold-runtime/task/hosts/junction
fixtures, and direct read-only preflight against the damaged installation. These
checks do not imply a service installation or reboot acceptance test.

The 2026-10-21 development-machine run passed 5 Rust recovery tests, 32
session-agent tests and 18 PowerShell checks. The checked recovery then removed
two orphaned tasks and three native-host registrations, archived the unchanged
authority database, and completed unprivileged browser/user cleanup. Subsequent
checks found no Focuser service/driver, tasks, owned registrations, shortcuts or
scoped user-data folders. The machine had no active commitment. No enforcement
was installed or started. Recovery diagnostics and tool hashes are recorded in
`target/windows-leftovers-recovery/validation.json`; reboot behavior remains
unverified. This is scoped application cleanup, not deletion of Windows event
history, development files or historical recovery backups.
