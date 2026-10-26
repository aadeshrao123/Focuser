# Authenticated authority updates

Phase 3 adds a service-owned update transaction. It does not run an installer or
activate persistent enforcement during development. The extension is unchanged.
The earlier direct-testing preference does not authorize activating phase 3 on
this machine: the current request explicitly defers OS acceptance.

## Supported contract

Only schema-neutral, stable-version Windows x64 updates are supported: product
`com.focuser.authority.windows-x64`, IPC v1, SQLite schema 8. A package must read
schema 8 and write schema 8. New schema migrations and trust-anchor/broker-protocol
rotation require a later coordinated release. Existing schema migrations now use
one SQLite transaction, including data transforms and schema-version records.

The fixed recovery broker is a private copy of the enrollment build. Its SHA-256,
installation ID and broker protocol are pinned in private `broker.json`. Repair
verifies this identity and does not silently replace the broker. New enforcement
binaries must preserve that broker's signing and protocol contract. Binary update
does not replace the uninstaller; the service IPC v1 removal contract remains valid.

The unsigned phase-2 installation cannot bootstrap its own trust into phase 3.
Ordinary reinstall and legacy `/UPDATE` still return `upgrade_deferred` before
extraction. A future trusted bootstrap, or an explicit administrator recovery/import
preserving the existing database, is required for that installation. There is no
`--force`, unsigned development update, arbitrary package path or updater identity
flag accepted by the authority.

## Two independent signature checks

The authority fetches only the fixed HTTPS `windows-authority-update.json` feed
under the project's GitHub releases. It verifies the exact manifest bytes with
the existing Tauri Minisign public key using the locked `minisign-verify` library.
The signed manifest binds product, version, security version/floor, protocol/schema
ranges, expiry, Windows signing identity and every payload filename/length/SHA-256.

Windows PE signing is separate. Each replacement executable and optional GNU
WebView loader must pass `WinVerifyTrust` and match the SHA-256 fingerprint of the
leaf Authenticode certificate compiled into the authority/broker through
`FOCUSER_AUTHENTICODE_SHA256`. Verification uses cached chain revocation checking
and no trust-verification network retrieval. Missing revocation evidence or an
unconfigured publisher pin fails closed. No test key is compiled as a production
trust anchor. NSIS release installers must also be Authenticode-signed; signing an
update manifest does not sign an installer.

Release credentials are absent here. The debug installer is unsigned and its
production update API returns `signing_unconfigured`. This is intentional.
`scripts/build-authority-update.ps1` requires already Authenticode-signed payloads
and an external Minisign signing-key file. It refuses unsigned or differently
signed binaries, writes the envelope and fixed payload files, and never installs
anything. Publish those assets only after release signing and OS acceptance.

Sources: [Tauri updater signing](https://tauri.app/plugin/updater/),
[Microsoft WinVerifyTrust flags](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/ns-wintrust-wintrust_data).

## Staging and limits

The update is a signed manifest plus five fixed binaries and an optional WebView
loader. There is no archive extraction or downloaded installer execution. No
downloaded command, client path, script or DLL search-directory argument is used.
Only HTTPS GitHub/release-asset hosts are accepted, including at each redirect.
Proxy environment variables are not used for privileged downloads.

Limits: 64 KiB decoded manifest, 192 MiB per file, 512 MiB total payload, three
redirects, 10-second connection and 90-second request deadlines, 10-minute staging
deadline, no automatic download retries, and one background check/download worker.
Space is checked for the payload, a bounded known-good backup and a safety margin.
Cancellation drains the current bounded request before conflicting maintenance
can proceed. Old owned caches are pruned using explicit filenames and UUID
directories; unknown content is retained with an error, never recursively deleted.

Private ProgramData directories reject reparse points and unprivileged access.
Files are created without overwriting existing entries. Atomic journal replacement
uses flush plus `MoveFileExW` with replace/write-through flags. The broker rechecks
the signed envelope and payload digest, holds a read handle denying write/delete
through verification and copying, and verifies the protected destination before
replacement. No unprivileged client can replace staged content between verification
and use. Administrators remain outside the tamper-resistance claim.

## Durable transaction and continuity

The journal records installation ID, single-use UUID, signed-manifest digest,
version/security floor, trusted signing identity, compatibility, current policy
revision, authorization expiry, checkpoint, known-good hashes and bounded attempts.
States cover checking, available, downloading, verifying, staged, deferred,
authorized, replacing, validating, rollback, rollback validation, completed,
cancelled/cancelling and recovery fault. Staging interruption is explicit.

The authority's maintenance mutex serializes update authorization, removal permits,
policy mutations and ticks. The private helper lock also serializes repair,
removal and broker execution. Staging permits ordinary policy operations and core
enforcement; a changed policy revision requires fresh authorization. Policy revision
is persistent; ordinary reads no longer increment it. Unknown IPC fields claiming
paths, keys, commands, unlocked state or updater privileges are rejected.

Activation is conservatively deferred for enabled manual protection, any enabled
scheduled protection (including schedule gaps), a retained list lock, enabled strict
allowances, or an active/paused Pomodoro. This restricts update timing; it does not
change list protection, release conditions, breaks, budgets or removal semantics.
The broker is launched only after authorization and must consume it through the
authenticated pipe as the actual pinned SYSTEM process within 60 seconds. Policy
and activation conditions are rechecked under the same authority lock.

After consumption, mutable accounting/commands are quiesced and a semantic digest
of every policy/history/settings table is recorded. Lists, standby/schedules,
Pomodoro state/pause deadlines, allowance consumption, private metadata and browser
grace deadlines remain in the authoritative database. When browser coverage ends,
the authority clears its grace record as the previous agent did; restarting a
component while coverage is active does not reset it. Browser grace deadlines now
belong to the service, so restarting an agent cannot refill the grace period.
No update backup is ever restored over that database.

The old service continues enforcing throughout download/staging. Once the replacement checkpoint is consumed, mutable accounting and process
enforcement are quiesced for backup, replacement and startup validation.
**Executable/path blocking pauses during that maintenance interval, including while
the authority process is stopped.** Existing
hosts entries and journaled browser restrictions remain, but are not equivalent to
complete process/title/foreground coverage. New activation is deferred during strict
commitments because this phase does not provide a second simultaneous process
enforcer. There is no claim of uninterrupted process enforcement across replacement.

The agent exits normally for update without restoring browser restrictions or
deleting journals/startup entries. Update does not require a healthy agent. The
fixed least-privilege task is started again after service validation; an absent
interactive owner remains an explicit degraded-coverage state. Uninstall retains
its independent healthy-agent cleanup requirement. Tauri closes only its own
interface after successful activation authorization; the service/broker own the job.
An already open production UI and native bridge also poll authenticated status and
exit their own process for authorized replacement, including CLI/silent requests.
Replacement retries Windows sharing/access conflicts for at most six seconds per
file, then fails and attempts compatible rollback; no other process is killed.
A browser can reconnect its native bridge after completion; an absent agent is
restarted through its independent task.

## Replacement, validation and recovery

A protected SYSTEM boot task runs the pinned broker even if Tauri and the agent
never open. It is idempotently provisioned and its identity is verified before
repair/removal. The authority refuses to open policy storage during incomplete
file replacement/rollback. Expected SCM stops do not invoke failure recovery.

All known-good binaries are saved and flushed before live replacement. Backup
failure leaves the old service running. Replacement is atomic per fixed filename;
the durable state enables rollback after a partial batch. Startup validation uses
the authenticated SCM/pipe endpoint and checks target runtime version, security
version, protocol/schema and unchanged update trust identity. Policy fingerprint
validation occurs before normal opening of the database; no replacement DB is
created. Rollback has its own startup-validation state and is not complete merely
because files were copied back.

Recovery restores compatible known-good binaries, never an old policy database.
An expired unused authorization cannot activate. A consumed transaction may still
verify its original signature for recovery after manifest expiry; this authorizes
rollback, not new activation. The security floor is monotonic, and incompatible
schema/security rollback produces an explicit recoverable fault. A healthy target
interrupted during final validation can finish rather than downgrade below a newly
committed floor. Attempts are bounded to the initial run plus one recovery run.

If a broker disappears before SCM stop, a tick in the original runtime proves that
replacement has not begun. The authority verifies the recorded broker PID is gone,
marks that no-replacement operation rolled back and resumes normal enforcement
without restoring or changing policy data. It also requests the owner agent again.
Other interrupted consumed states remain frozen for broker recovery.
Expiry never marks incomplete replacement healthy. Private `broker-error.json`
retains failure details. A failed stage can be cancelled and checked again; consumed
maintenance requires recovery. UI loss does not cancel the worker or journal.

## Entry points and remaining acceptance

The UI and `focuser-cli service-update` share status/check/stage/activate/cancel
requests. Activation/cancellation name only a service-issued transaction ID. The
installed service helper's `update-request` advances the next applicable step.
NSIS `/REQUEST_UPDATE`, interactive or `/S`, materializes its bundled IPC requester
in newly created private BA/SYSTEM-owned objects under Program Files. The directory
and empty destination file are created atomically with explicit security descriptors
before extraction, avoiding execution of an unverified older installed helper. It
requests the live authority and removes only that private requester, then exits
before any installed-file extraction or fresh provisioning. Exit 0 means request
accepted, not update completed; query service status. This payload is not an update
package and cannot supply a replacement path or signing authority. Legacy direct installer replacement remains unsupported.

The UI displays progress, staged/deferred status, installation, rollback and recovery
codes. A disconnected service is a diagnostic failure, not permission to update or
open a writable local database. Removal still requires its own short-lived,
single-use permit before stopping/cleaning anything; data retention is unchanged.

See `service-phase3-acceptance.md` and the validation report for tests performed and
OS checks not yet verified. Production release signing, MSVC release/OS acceptance,
legacy bootstrap, schema-changing updates, trust/broker rotation and broader
multi-session coverage remain separate work.
