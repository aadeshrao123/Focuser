# Service-owned file deletion protection

The normal Windows SCM authority now holds Windows sharing locks during the
same stored `prevent_uninstall` commitments used by ordinary removal. This is a
working user-mode layer, not the completed administrator-resistant kernel design.
It must not be described as administrator-proof or as protection for every file
on the computer. The experimental minifilter remains undeployed and gated.

## Policy and ownership

Only the production service attaches the verified installed executable directory
and its authoritative storage directory. Foreground/development clients do not
attach live production roots. No new IPC command accepts a path, unlock flag,
administrator assertion, or request to close protected handles. Protection status
is read-only through `GetSetting("_authority.file_guard")`; its JSON explicitly
reports `administrator_resistant: false` and `idle`, `protected`, or `degraded`.

The existing service removal decision covers all relevant lists and their original
expiry/unlock conditions. Manual commitments survive pauses, Pomodoro breaks and
schedule gaps. Scheduled locks retain their existing occurrence/gap semantics.
Existing opt-outs are preserved; a list without uninstall protection does not
silently acquire a stronger commitment. Closing Tauri does not own or close these
handles. Restarting the authority reacquires them from the stored commitments
without starting a new timer. Expiry/unlock releases locks on synchronization;
ordinary idle ticks do not transiently relock an unprotected installation.

Handles are acquired before acknowledging policy writes. A missing required
payload, conflicting existing delete/write handle, reparse point, or object limit
prevents a new protection command from being committed. Partial acquisition retains
the handles it already obtained and returns a diagnostic. An unreadable stored
policy does not authorize release. New owned files discovered during a protected
tick are added to coverage. Enumeration is bounded to 4096 objects.

The shared database list reader now returns an error for an unreadable or malformed
row. It previously skipped those rows, which could turn a corrupt protected list
into an apparently empty/unprotected snapshot. A failed refresh retains the prior
engine cache and held file locks. Source data is not deleted or rewritten to hide
the error.

## Covered operations and compatibility

Every existing regular file and directory under the installed payload is opened
without delete sharing. Installed regular files also deny write sharing. Ordinary
delete, rename and overwrite of installed names are rejected by Windows sharing
checks, independently of whether the caller has an elevated token. An outside
hardlink name may still be unlinked on modern NTFS; its deletion does not remove
the pinned installed name. Writes through an existing alias are tested separately.
Read/list access is intentional: attribute-only opens do not establish the same
sharing restriction. Reparse objects are rejected rather than traversed.

The authority directory and main SQLite database deny delete sharing while allowing
ordinary database writes. `service.json` and a provisioned recovery broker/pin are
pinned. Optional task XML inputs remain writable for same-code repair, but cannot
be renamed/deleted while pinned. First enrollment can create its broker/task inputs;
the service adds them when available. Existing native-host manifests are not opened
for writing during identical repair. A real content change still needs ordinary
filesystem authorization; there is no file-lock bypass for an installer flag.

SQLite WAL/SHM, rollback journals, logs, update staging, browser profiles and user
interface caches are deliberately not locked. SQLite must be able to replace or
delete its transient files and checkpoint correctly. The existing private storage
ACL remains necessary. Other applications and browser updater files are outside
the protected roots.

Removal keeps the original preparation/consumption/revision checks. A protected
decision denies before cleanup; after legitimate release and authorization the
service does not retain these sharing locks, so the existing cleanup can proceed.
No general updater, administrator, or forced-unlock exception was added.

## Limits and acceptance

A determined administrator can terminate the owning service, close/duplicate its
handles with sufficient process rights, alter writable authority contents, manipulate
the filesystem through other privileged mechanisms, or work offline. All service
handles also close during OS shutdown and a service restart; there is a coverage
gap until the restored authority attaches them again. This layer cannot enforce
the kernel prototype's continuity guarantee during that interval. Existing explicit
service-stop settings and administrator recovery are preserved.

Scheduled boundaries and newly created files are observed on the service tick.
There is no claim of nanosecond boundary protection or coverage of a file created
and removed between enumerations. Task registration itself, registry service keys,
all SQLite transient data and pre-existing mapped writes are not secured by these
sharing locks. These are reasons a signed, validated kernel design is still needed
for stronger guarantees, not reasons to enable the experimental driver.

Automated tests operate only on temporary files and databases. They exercise
Windows deletion/rename/replacement denial, hardlinks, unrelated updater files,
SQLite WAL/checkpoints, overlapping commitments, disabled break state, expiry,
verified/wrong unlock, failed acquisition, corrupt stored state, newly created
payload files, startup/restart, and same-code repair inputs. The separately opt-in
`elevated_cleanup_is_denied_while_the_owning_service_object_lives` test uses an
actual elevated PowerShell child to attempt deletion on dummy scratch payloads;
it does not register/start any service or load a driver. Elevated results must be
reported separately from the ordinary suite.

Remaining live acceptance: full SCM provisioning, UI termination, real installer
interactive/direct/silent denial and post-expiry cleanup, normal shutdown/reboot,
antivirus and filesystem compatibility, and recovery after partial installation.
Do not infer these results from scratch-file tests. No live installation or
persistent enforcement is authorized by running the automated suite.

Reference: [Windows CreateFile sharing semantics](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).

## Executed validation and test installer (2026-10-20)

* Normal release-mode core/service suite: 184 passed, including ten new Windows
  file-guard tests. The elevated test was explicitly ignored in that run.
* Separate corrupt-row integration regression: one passed (invalid JSON and
  invalid SQLite row type both preserve the old complete policy snapshot).
* Separately executed elevated scratch probe: one passed using an actual elevated
  Windows token. Delete, directory rename and replacement failed while the dummy
  payload was held; unrelated deletion and post-release deletion succeeded.
* Release service build, formatting, PowerShell parsing and whitespace checks passed.
* The Rust service also passed `cargo check` with `kernel-file-guard` enabled.
  This checks integration compilation only; no WDK driver build/load was performed.
* The actual rendered NSIS installer passed authorization/order inspection, all
  13 negative installer fixtures, and the file/nested-resource permission checks.

New unsigned test installer:
`D:/Focuser/Focuser-Setup-FILE-PROTECTION.exe`

SHA-256: `7495FD86A362B5B35BCD9C2D333598686F411C6CC48D32217A296A60CCCF347A`

Service SHA-256: `2101C717BBE7E9A423E4D0B70C57BA30A4AA5217222F1AC6854D9DCD927B043B`

The service payload was rebuilt; the previously fixed browser agent, UI, CLI,
native bridge and loader were preserved. The prior service payload was archived
in `C:/Users/Ahmed Ameen/AppData/Local/FocuserDevTools/file-deletion-protection-20261020`.
That directory contains the rendered installer, test log and validation metadata.
`Focuser-Setup-CURRENT.exe` remains the earlier browser-update candidate; the new
file-protection artifact has its own explicit name. No setup was executed.

The current machine had no registered authority at verification. Saved production
data and prior recovery archives were not modified. The normal installer's retained
enrollment/repair/version checks remain in place: this test installer is not an
unsigned upgrade bypass or a missing-payload bootstrap. Installing over retained
older state still requires the existing authorized maintenance/recovery procedure.
No driver, signing policy, SCM entry or persistent strict enforcement was activated.
