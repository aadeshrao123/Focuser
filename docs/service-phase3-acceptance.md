# Phase 3 Windows acceptance — not executed

The current implementation task explicitly forbids installing or activating
persistent enforcement on this development machine. No VM is required to finish
implementation; VM acceptance remains deferred. If the user later elects direct
testing, these are deliberate operator checks, not scripts to run automatically.

1. Preserve a consistent SQLite backup including WAL, existing policy/usage,
   ProgramData metadata, current binaries, task XML, SCM configuration and hashes.
   Verify the explicit administrator recovery procedure before starting. Use short
   bounded test commitments; never clear real commitments to make testing easier.
2. Supply controlled release credentials outside the repository. Compile the
   publisher certificate SHA-256 pin, Authenticode-sign every payload, sign the
   manifest with the matching Minisign key, and Authenticode-sign NSIS separately.
   Confirm unsigned GNU/debug packages cannot stage or activate as updates.
3. Enroll a fresh phase-3 installation, retaining/importing all existing data as
   explicitly selected. The unsigned phase-2 installation has no authenticated
   bootstrap path; ordinary reinstall and raw `/UPDATE` must continue to fail
   before extraction. Do not silently replace it with an empty authority.
4. Inspect Program Files/ProgramData/SCM/task ACLs using a second standard account.
   Verify protected broker hash/installation binding, SYSTEM boot-task principal,
   fixed executable/arguments, task identity validation, single active job and
   rejection of junctions, renamed/replaced files and writable parent paths.
5. Check updates from UI, CLI and `/REQUEST_UPDATE` with and without `/S`. All must
   reach the same service. Malformed identities/paths/keys/unlocked claims, altered
   manifests/files, wrong product/version, bad publisher/revocation state, expired
   manifests and security downgrades must fail before replacement.
6. Stage while protected lists overlap, during schedule gaps and pauses, during
   Pomodoro work/break/pause, and after using individual/shared budgets. Verify
   enforcement continues, activation defers as documented, every policy change
   invalidates authorization, and no state/usage/grace deadline is reset. Closing
   or terminating Tauri must leave the service-owned job intact.
7. Activate only in the permitted non-strict window. Observe the honest process
   enforcement gap while SCM is stopped. Hosts/browser restrictions must remain.
   Verify normal agent exit preserves recovery journals, new/old agent protocol
   compatibility, restart without Tauri, absent-agent behavior and logon recovery.
8. Inject failures at every durable boundary: backup, journal flush/rename, each
   file replacement, service startup, new health validation, floor commit, old-file
   restoration and rollback validation. Repeat with disk full, truncated downloads,
   held-open binaries, unavailable authority and an occupied/spoofed pipe. Failed
   backup must not stop the old service. Partial failures must never report success.
9. Deliberately interrupt/reboot at each consumed-maintenance boundary. The SYSTEM
   recovery task must run without the user/agent/UI, restore only compatible binaries
   and preserve authoritative data. It must stop retrying with a visible recovery
   fault. Rollback below the security floor or into an incompatible schema must be
   refused. A consumed expired transaction may recover; an unused expired permit
   cannot activate. Confirm a completed rollback can restart again after later usage.
10. Repeat protected interactive/direct/silent uninstall tests, including pending
    updates, missing agent and unavailable service. No cleanup precedes removal
    authorization. Successful removal must delete only the verified Focuser recovery
    task in addition to the existing owned resources, honor data retention, and
    never restart components through stale recovery jobs.

## Explicit administrator recovery for a damaged update

Record `updates/journal.json`, `broker-error.json`, floor, binary hashes and actual
SCM state first. Follow phase 2's explicit stop/reboot/backup procedure; temporarily
disable only Focuser's recovery task and set only its service to Manual. Do not
disable Windows recovery or remove unrelated enforcement resources.

With the service stopped, preserve the entire authority directory and consistent
SQLite/WAL data in an administrator-protected backup. Restore only a cryptographically
verified known-good binary set that can read the current schema and is at or above
the existing security floor. Do not lower that floor, restore stale policy data,
delete commitments, reset usage, or treat an absent pipe as uninstall permission.
For a schema that has advanced incompatibly, use a compatible forward-repair release;
an older binary/database pair is not an acceptable rollback.

After validating the recovered binary/schema/data combination, an administrator may
archive the update journal and record its resolved transaction as `rolled_back` with
an explicit recovery reason, preserving installation identity and policy revision.
Inspect any independent uninstall journal separately. Restore Automatic service
startup and the verified Focuser recovery/owner tasks, then verify commitments,
budgets and coverage before normal use. This is explicit operator recovery, not a
client bypass flag or an automatic unavailable-service fallback.
