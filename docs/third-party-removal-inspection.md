# Third-party cleanup inspection — October 19, 2026 (Hawaii)

This inspection copied retained data and queried Windows state. It did not
install/start services, change accounts or ACLs, modify policy, remove third-party
software or change the browser extension.

## Observed state

- `C:\Program Files\Focuser` is absent. All installed clients, service executable,
  session agent, native bridge, uninstaller, loader and native-host manifests in
  that directory are consequently missing.
- `FocuserAuthority` is no longer registered and no Focuser process is running.
  Enforcement is currently unavailable.
- The machine uninstall registration is absent.
- A **targeted** `schtasks /Query /TN \Focuser\SessionAgent /XML` succeeds. The
  existing task points to the missing executable. An initial broad task inventory
  missed this registration; it must not be treated as evidence the task was deleted.
- Machine Chrome, Edge and Brave native-host registration keys remain, referencing
  an installation whose files are missing. The Firefox key is absent.
- The ProgramData authority directory still has its expected private ACL:
  SYSTEM and Administrators full control, no ordinary-user access. No permissions
  were loosened to read it; an elevated read-only diagnostic was used.
- The database passes SQLite integrity checking. It contains the original list
  ID, now with **414** website rules, plus statistics, blocked-event history and
  Pomodoro records. Its list is enabled, with no weekly schedule. The stored
  manual protection expired at **05:23:43 Hawaii time** on October 19, before the
  observed cleanup. An enabled blocking list is distinct from an unexpired
  uninstall-protection commitment. No expiry was changed during inspection.
- The maintenance marker is empty and no removal progress/completion records
  exist. There is no recorded completed service-authorized uninstall transaction.
- The real Windows token of the still-running `BCUninstaller.exe` process reports
  **elevated=true**, for the enrolled user. `ASCService.exe` separately reports
  elevated=true under LocalSystem. No new UAC prompt is required to establish
  that an already-running process has administrator privileges. This does not
  establish which process performed each individual deletion.
- SCM events record two unexpected service terminations followed by startup being
  changed from automatic to disabled. Later inspection finds the registration
  absent. The events do not by themselves attribute those actions to a caller.

## Preservation and checks

A private copy of all retained non-database authority files and a consistent
SQLite online backup were made under:

`C:\ProgramData\Focuser.DamageInspection-378ffcb5a827453ca684632a4c028ddf`

The backup helper checked SQLite integrity and all table fingerprints against
the source transaction. Original data, including the maintenance marker, was
not edited. This latest backup includes the user's changes from 320 to 414
website rules; an earlier reinstall backup must not replace it silently.

Sanitized diagnostics and the read-only inspection scripts are in:

`C:\Users\Ahmed Ameen\AppData\Local\FocuserDevTools\removal-damage-inspection-20261019`

The existing rendered-installer permission test passed again: ordinary Users
receive read/execute without write/delete/ACL-modification rights on installed
files. This checks the generated permission commands on scratch objects; deleted
Program Files objects cannot be retrospectively inspected for their actual ACLs.

## Protection boundary and practical solution

The service gate protects supported uninstall requests. It does not intercept
arbitrary elevated filesystem, process or SCM operations. The installed ACL and
SCM descriptors deliberately retain administrator recovery access. No prompt
does not mean an operation was unprivileged; the process token above is direct
evidence of elevation.

Windows administrators can manage service security and take ownership of files.
Restricting ordinary users does not create protection against an administrator.
See Microsoft's [service access-right documentation](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights)
and [ownership policy documentation](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/take-ownership-of-files-or-other-objects).

The practical deployment for this requirement is a standard daily-use account
with administrator credentials controlled separately during commitments.
Administrator-level cleanup utilities and any privileged helper interfaces must
also be inaccessible to that account. Account conversion/management is an
explicit administrator decision; this investigation did not change accounts or
remove unrelated utilities. Adding a watchdog, deny ACL or code signature would
not make the present application administrator-proof.

Recovery requires restoring matching, verified installation files and SCM
registration while retaining this latest database and installation identity.
The existing administrator recovery procedure in `service-phase2.md` applies.
Do not initialize an empty authority or import an older backup over these rules.
Repairing files alone will not prevent another elevated third-party cleanup.
No automatic repair or persistent enforcement was activated by this inspection.
