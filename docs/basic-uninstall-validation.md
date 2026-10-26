# Basic uninstall protection completion — 18 October 2026

This change finishes the existing removal path. No extension source changed, no
new protection system was added, and no installer or persistent enforcement was
activated on this development machine.

## Fixes

- Stored manual uninstall commitments remain effective while blocking is paused
  or disabled for a Pomodoro short/long break. Restart preserves the original
  start and expiry. Existing scheduled-occurrence gaps and verified occurrence
  unlocks retain their previous semantics.
- An uninstall-only commitment cannot be erased by deleting its list, resetting
  or importing configuration, or re-arming protection with weaker flags or an
  earlier expiry. A verified unlock still works during a break.
- The service names every blocking commitment and reports its expiry or required
  unlock. The installed helper returns that diagnostic; NSIS displays it for
  interactive removal and records it in silent uninstall details.
- Interactive, direct and silent removal share the existing service-authorized
  permit/consume/drain path. Installer file cleanup remains after that barrier.
  An unreachable authority returns a bounded error without cleanup.
- After consumed authorization and agent cleanup, the service asks the UI and
  native bridge to exit cooperatively. A mere allowed decision never closes
  clients. The helper waits for owned clients before stopping SCM, and reports
  incomplete removal if a client stays open. No process-killing heuristic was
  added. Only verified Focuser native-host registrations are removed, after
  authorization, while IPC can still deliver shutdown to existing clients.

Primary source changes are in `focuser-common/src/types.rs`,
`focuser-app/src/execute.rs`, `focuser-ipc/src/{lib,protocol}.rs`,
`focuser-service/src/{authority,removal,installation,main}.rs`,
`focuser-ui/src/windows_main.rs`, `focuser-native/src/main.rs`, and the NSIS
template/audit scripts. Paths above are relative to `crates/` unless under
`scripts/`. Relevant tests are the service authority, removal and Windows-pipe
integration suites plus installer-audit mutation tests.

## Automated validation

All 444 Rust unit/integration tests passed; strict Clippy also passed. The final
Rust test and strict Clippy logs are
`target/uninstall-basic-tests-verified.log` and
`target/uninstall-basic-clippy-verified.log`. The run covers common, core, app,
IPC, service, CLI, native bridge and session agent, with all targets and Clippy
warnings denied. Frontend `npm run typecheck` passed
(`target/uninstall-basic-typecheck.log`).

Focused coverage includes overlapping commitments; expiry and verified unlock;
denial before cleanup in the shared interactive/direct/silent transaction;
re-arming/reset/import/deletion denials; original expiry across authority reopen;
Pomodoro pauses and both break types; and shutdown only after consumed, ready
authorization. Existing permit replay, expiry, policy-race and interrupted
maintenance tests remain in the suite.

Real Windows development named pipes were tested with scratch storage and
enforcement disabled. Terminating an exact disposable test-client process leaves
the independent authority clock, rules and commitment intact; reopening its
database preserves the original expiry. A missing development pipe produces
`service_unavailable` within the timeout and never invokes cleanup. This is a
client-process/authority harness, not an installed Tauri/SCM acceptance test.

The GNU Windows debug NSIS installer built using pinned Tauri CLI 2.10.1. The
actual rendered installer was inspected: both retention branches invoke the
service helper, capture its result and diagnostic, and abort with exit code 26
before file deletion on failure. All 11 installer-audit regression mutations
were rejected, including a missing removal diagnostic. Rendered permission
commands passed file and nested-resource ACL harness checks. Only the existing
protected per-machine NSIS package is supported by this removal implementation.

Artifact: `target/basic-uninstall/Focuser_0.10.0_x64-basic-uninstall-debug-setup.exe`
(unsigned development build; not installed).

SHA-256:
`4B51E9837AC2ACED70E062DA239C5D2C74D22FC6CC412B21473E55842CBCC0BA`

The rendered script and build/audit log are alongside the artifact as
`installer.generated.nsi` and `build-and-audit.log`. Existing phase artifacts
were preserved.

## Deferred Windows acceptance and limits

VM testing remains deferred. No live SCM registration, startup task, system
hosts/browser-policy modification, installed uninstall or reboot was performed.
Consequently these checks remain **unverified**:

1. With an installed protected commitment, interactive, direct and `/S`
   uninstall each return failure and the blocker/release diagnostic, leaving
   the service, tasks, files, registrations and policy unchanged.
2. Terminating the actual Tauri interface leaves installed executable/path and
   browser enforcement active. Restarting the SCM service and rebooting retain
   commitments and their original expiry.
3. Pauses and both Pomodoro breaks deny installed removal; overlapping
   commitments deny until every relevant blocker expires or is verified-unlocked.
4. After release, installed removal closes clients, restores only owned browser
   resources, removes owned components, and honors both data-retention choices.
5. With the installed authority unreachable, ordinary uninstall reports the
   error and performs no installer cleanup. Partial cleanup reports failure.

The independent enrolled session agent is still required for browser restoration;
missing/stuck agents fail removal rather than skip restoration. Single-policy-
owner SID and existing schedule-occurrence semantics remain. The explicit
[administrator recovery procedure](service-phase2.md#interrupted-maintenance-and-administrator-recovery)
is retained for damaged installations. This does not claim resistance to a
determined administrator. Existing update work was preserved without expansion.
