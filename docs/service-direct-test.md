# Direct Windows testing and incomplete-installer recovery

On 18 October 2026 the user chose direct testing on this machine, without a VM.
This supersedes the VM prerequisite in earlier acceptance documents. Remaining
SCM/UAC/ACL/logon/reboot checks are still unverified until actually exercised;
the old checklist is coverage guidance, not evidence of a completed test.

## Installer permissions defect

The first direct installation extracted six payload files and then failed before
creating ProgramData authority storage, registering a service, or creating the
agent task. A second attempt returned `upgrade_deferred` because the incomplete
Program Files directory still existed.

The cause was `/inheritance:r /grant:r ...:(OI)(CI)... /T`: applying container
grants recursively left regular files with an empty effective DACL. This was
reproduced on an owner-controlled scratch file. The provisioning helper could
not execute. The installer now grants permissions to the root directory only,
then resets its children to inherit that protected ACL. Both commands must
succeed before the helper runs. Ordinary users receive read/execute; only
SYSTEM/Administrators receive write permissions.

`scripts/test-installer-permissions.ps1` exercises those exact rendered commands
on scratch files and nested resources, checking readability, inheritance and
the absence of Users write/delete/ACL rights. The rendered-script mutation tests
also reject recursive container grants or a missing child reset.

## Recovery performed

`scripts/recover-incomplete-installer.ps1` first verified the absence of authority
storage, SCM registration, startup tasks and uninstall registration, and verified
that the directory contained only the six expected payload files. Its elevated
`-Apply` mode restored protected readable permissions and moved the incomplete
directory to:

`C:\Program Files\Focuser-Incomplete-7d78ee55-d7a0-4b53-ad43-89781f92209e`

No files or policy data were deleted. No service or persistent enforcement was
started by this recovery. The script refuses an enrolled/registered installation;
it is not an ordinary removal or update authorization bypass.

## Subsequent missing-payload diagnostic

Read-only inspection after the reported `configuration: An absolute installed
executable is required` error found that `C:\Program Files\Focuser` was absent,
while `C:\ProgramData\FocuserAuthority` existed and denied access to the ordinary
development account. The earlier archived payload was still present. No service
named FocuserAuthority was visible in SCM. The private directory's contents,
commitments and enrolled binary version were not inspected or changed.

The generic error conflated relative paths, missing files and inaccessible files.
Installed-file checks now report the exact path and filesystem error. Repair
reports `repair_payload_missing` when its installed service file is missing,
preserves all retained data and points to administrator recovery. Scratch tests
check this diagnostic and verify that no replacement directory/data is created.

This is a damaged-installation condition, not the earlier pre-provisioning-only
condition. Do not use `recover-incomplete-installer.ps1 -Apply`, delete private
storage, or choose empty initialization to work around it. Follow the explicit
administrator recovery procedure in `service-phase2.md`, preserving the full
private directory and restoring the original trusted build. Ordinary same-code
repair cannot bootstrap an unknown enrolled version. This follow-up did not run
an installer or restore/start any persistent component.

## Continue direct acceptance

### Empty abandoned setup recovered after the missing-payload screenshot

An elevated read-only SQLite check subsequently established `quick_check=ok`,
zero block lists, allowances, active blocks, Pomodoro sessions, usage and history,
and schema version 8. The two settings were the installation identity and the
original `autostart_enabled` preference. There was no SCM authority registration
or installed program directory. Matching native-host entries and the original
least-privilege, owner-bound SessionAgent task remained; their paths, enrollment
identity and task principal were verified and they were left unchanged.

`scripts/recover-empty-abandoned-setup.ps1` is an **explicit administrator
recovery utility**, not an installer hook, client command, update bootstrap or
ordinary uninstall exemption. Its default is a dry run. It refuses any policy or
history rows, unknown schema/settings/files, SQLite journals, pending coverage or
maintenance, mismatched integration, active storage locks, installed binaries or
an SCM authority. Apply additionally requires the inspected installation ID and
database SHA-256. It never starts a service or creates a replacement database.

The applied recovery moved the entire private directory atomically to:

`C:\ProgramData\FocuserAuthority.Recovery-e981f163176545c088a25c43bfe60102`

All six original file checksums matched after the move. The directory retains its
private ACL; an ordinary-account listing was still denied. Nothing was deleted.
The database SHA-256 remained
`45AD43EA9297556C6F58EEE553666392435EC4E32AD0EE736FB703F938066AFD`.

The user can now run
`D:\Focuser\target\windows-repair-diagnostic\Continue Setup.cmd`.
Its PowerShell launcher verifies the unchanged diagnostic installer's SHA-256,
then opens setup with the original owner SID and an explicit import of the
archived database. It never selects `/EMPTY`. Review that import in the wizard.
This preserves the installation identity and preference and lets normal
provisioning reuse the matching task. Neither the launcher nor installer was
executed as part of this recovery. No new enforcement was activated.

Thirteen new scratch SQLite recovery tests passed, covering retained policy,
history, preferences, unknown schema/settings, maintenance, pending coverage,
journals and missing/corrupt sources. PowerShell parsing, real dry-run ownership
checks and the applied six-file checksum/ACL verification passed. Existing Rust
and installer results are the unchanged baseline from the diagnostic build;
SCM provisioning, logon/reboot and installed enforcement acceptance remain
unverified. `recovery-receipt.json` and test logs sit beside the launcher.

1. Use the corrected ACL-fix installer referenced by the validation report. Keep
   the incomplete-payload archive and source profile until installation is verified.
2. The wizard must enroll the intended unelevated user's SID (`whoami /user`).
   Explicitly import any existing policy database. Choose empty initialization
   only when there are no commitments/history to preserve. Never silently replace
   existing authority data or clear a maintenance journal to get setup to proceed.
3. Retain the installer Details output, especially any helper error and exit code.
   Verify Automatic/LocalSystem SCM configuration, ProgramData/file permissions,
   the least-privilege owner task and visible agent coverage before making a test
   commitment. Verify that UI closure leaves the authority running.
4. Start with bounded, short test commitments and retain a consistent SQLite
   backup before data-changing tests. Check ordinary and silent removal denials
   before testing authorized cleanup and the explicit retention choice.
5. Check restart/logon/reboot behavior as separate, deliberate tests. Do not run
   the old power-loss, corrupted-installation or destructive fault-injection steps
   automatically on this working machine. The explicit administrator recovery
   procedure remains in `service-phase2.md`.

The extension is unchanged. Secure binary updates remain deferred. Successful
installer compilation or permissions probes do not establish full OS acceptance.
