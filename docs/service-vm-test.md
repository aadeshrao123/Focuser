# Disposable Windows VM acceptance procedure

**Current testing choice (18 October 2026): the user chose direct testing on this
machine without a VM. Follow `service-direct-test.md` first. The historical VM
requirement below is superseded; the checklist remains reference coverage only.**

**OS-level acceptance is not established by unit tests. Run this procedure in a
snapshot-capable disposable Windows 10/11 VM, never on an everyday machine.**
No real SCM installation, persistent login task, reboot, system hosts change or
browser-policy activation is performed by the automated scratch harness.

For phase 2, use the **Phase 2 acceptance** section below with the NSIS package.
The manual setup that follows is historical phase 1 coverage and does not validate
the installer. Use the installed helper's bounded SCM recovery and independent
least-privilege task provisioning for phase 2.

1. Snapshot a clean VM. Create a standard local user `FocuserTest` and a separate
   administrator. Build/test the workspace with the supported Windows Rust
   toolchain and WebView2. Copy service, session-agent, native, UI and CLI release
   binaries into `C:\Program Files\Focuser`. In an elevated PowerShell:

   ```powershell
   $bin = 'C:\Program Files\Focuser'
   icacls.exe $bin /inheritance:r
   icacls.exe $bin /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX'
   icacls.exe $bin /setowner '*S-1-5-32-544' /T
   ```

   Inspect descendants: remove any inherited/explicit nonadmin write grants or
   reparse points before continuing. Application binaries and their parent must
   be immutable to the standard test user. Obtain that user's SID using
   `whoami /user` in their session, not the administrator's SID.

2. In a **legacy scratch profile/database**, create domain, keyword, path,
   executable and window-title rules, schedules, a timed pure commitment, a
   password lock, daily domain/app quotas, a spent shared allowance and Pomodoro
   history. Record identifiers, usage and end times. Close the legacy program.
   Keep a VM backup of its DB/WAL/SHM. Use the explicit provision/migrate commands
   in [service-authority.md](service-authority.md). Compare all tables/rows in the
   migrated DB with the source, including usage anchors/history. Try corrupt
   input, an existing destination, a read-only destination, insufficient space
   and interruption before publication. Confirm original data remains usable
   and no partial destination becomes authoritative.

3. Inspect ProgramData ACLs (`icacls`); ordinary `FocuserTest` must be unable to
   read/write/rename/delete DB, configuration, lock, WAL/SHM or parent directory.
   Deliberately loosen a VM copy's ACL/add a junction: startup must refuse it.
   Try replacing the agent binary as the standard user: access must be denied.

4. Register the service **in this VM only**, elevated:

   ```powershell
   sc.exe create FocuserAuthority binPath= '"C:\Program Files\Focuser\focuser-service.exe" service' start= auto obj= LocalSystem
   sc.exe failure FocuserAuthority reset= 86400 actions= restart/5000/restart/10000/restart/30000
   sc.exe start FocuserAuthority
   sc.exe queryex FocuserAuthority
   ```

   Configure one interactive login task for `focuser-session-agent.exe` for
   **FocuserTest**, at normal user privilege, independent of Tauri. Use Task
   Scheduler's interactive-token logon mode, not SYSTEM/Session 0. Do not use the
   old UI elevated task as the enforcement authority. Sign in as FocuserTest and
   start the agent and UI. Verify the agent is in the user's session, service in
   Session 0, and only the agent owns TCP 17549.

5. Start timed protected blocking. Verify UI and default CLI reject list deletion,
   settings weakening, budget reset and incorrect unlock with the same `protected`
   or existing core code. A crafted command with `unlocked:true`, fake identity,
   role or successful-challenge flag must not bypass checks. Valid lock responses
   must be checked by the service. The HTTP browser API must not accept `/command`,
   shutdown, arbitrary settings or protection-control messages. Use the unchanged
   extension to verify compiled rules, add/remove operations, private-window
   detection, usage ticks, popup data and block-page behavior.

6. Terminate **only** `focuser-ui.exe` in Task Manager. Start a blocked test
   executable/path; verify the service still terminates it. Leave the UI closed
   across a schedule boundary, a Pomodoro transition and allowance exhaustion.
   Confirm state changes and sound/notification from the independent agent;
   reopen UI and CLI and compare state. Remove/disable the unchanged extension:
   profile detection should trigger the existing installation-only browser
   recovery at its 500 ms polling cadence, subject to OS/browser restart latency.
   Normal browsing resumes only after coverage is re-established. Do not promise
   zero-latency browser removal detection.

7. Terminate the agent while the service runs. After the observation lease,
   confirm the UI displays degraded coverage, executable/path blocking continues,
   and active web/strict foreground quota targets fail closed. Window-title-only
   observation and desktop feedback are expected unavailable. Restart the
   protected agent; verify new observations restore coverage and interrupted
   browser-recovery journals are handled. Closing/reopening Tauri must not spawn
   duplicate agents or own their lifetime.

8. Test the named pipe from another standard Windows user, network access, a
   same-user executable claiming to be the agent, malformed JSON, unknown
   versions, zero/oversized lengths, truncated frames, idle clients and more than
   16 simultaneous connections. No unauthorized mutation may occur. Test a fake
   pipe server while SCM is stopped: production UI/CLI/native must reject its
   identity before disclosing a command/password. Check no writeable profile DB
   appears while disconnected. Close/kill the authority in the VM using admin
   tools; UI and CLI must show disconnection, never start a local engine.

9. With a protected commitment and used budgets, request SCM stop as the VM
   administrator: policy STOP should be refused while required. Reboot the VM
   normally; no recovery/shutdown mechanism is disabled. Automatic SCM startup
   must restore the same commitment expiry, allowance totals, shared usage and
   Pomodoro state before UI startup. Verify user-session observations return only
   after login. Administrators can forcibly terminate/remove the service; that
   remains outside phase 1's protection claim.

10. Save evidence (OS/build version, service token/session, ACLs, denied requests,
    DB comparisons, test timestamps). Restore the VM snapshot afterward. Do not
    reuse these test commitments/registrations on the host. Installer uninstall
    gating and authorized update packaging are the next deployment task.

Scratch automated checks:

```powershell
cargo test -p focuser-common -p focuser-core -p focuser-app -p focuser-ipc -p focuser-service -p focuser-cli -p focuser-session-agent
cargo check -p focuser-service -p focuser-session-agent -p focuser-native -p focuser-cli -p focuser-ui
```

## Phase 2 acceptance — deferred, disposable VM only

Do not execute this section on the development machine. Use the built per-machine
NSIS artifact and snapshot a Windows 10/11 VM with WebView2 already installed.
Record artifact hashes, OS build, elevation identities and exact exit codes.
Read `service-phase2.md` first. None of these OS checks is claimed verified by
unit tests or a successful installer build.

1. **Generated installer inspection.** Retain the actual rendered `installer.nsi`,
   compiler log and EXE hash from the pinned CLI 2.10.1 build. Run the audit script.
   Inspect `.onInit`, all sections, `un.onInit` and all uninstaller branches. Verify
   that existing-install and `/UPDATE` rejection occur before copying files,
   WebView/download actions, process checks or invoking an older uninstaller.
   Confirm the first uninstall side effect is the service helper, and that its
   nonzero/timeout branch aborts before every installer deletion. Verify there is
   no MSI, portable Windows executable or automatic updater ZIP in release output.
2. **Fresh install with two elevation identities.** Install with the same-user
   split-token UAC case and again, from a snapshot, with a standard policy owner
   and separate administrator credentials. Explicitly enter the owner's SID.
   Import a profile with protected lists, quotas, schedules, shared budgets,
   active/paused Pomodoro and history. Compare all state, preserving source WAL.
   Test a corrupt source and invalid SID: installation must fail without replacing
   the source or silently creating an empty policy. Verify `/EMPTY` is explicit.
3. **ACLs and SCM.** Inspect owner/DACLs on installed files, parents, native JSON,
   ProgramData/config/DB/WAL/SHM/lock/task XML, SCM service and scheduled task.
   Standard users must not replace, rename, delete or reconfigure authority code
   or storage. Other standard SIDs cannot use the pipe. Elevated non-owner callers
   cannot execute normal policy commands. Check service Automatic/LocalSystem,
   restart actions 5/10/30 seconds then NONE, and 24-hour reset. Do not change
   Windows recovery configuration. Check endpoint authentication under the real
   SYSTEM service, including a fake user-created pipe and remote connection.
4. **Provision and repair twice.** Run installed `install --repair --owner-sid SID`
   twice with existing protected state. Verify only one SCM entry and one owner
   task, unchanged installation identity/data/usage, least-privilege token and
   identical binary paths. Another owner SID or redirected agent path must fail.
   Re-run the installer normally, silently and with `/UPDATE`: it must return
   `upgrade_deferred` before changing any installed-file/data hash or stopping
   the service/agent. This phase does not support binary replacement upgrades.
5. **Independent session agent.** Reboot and log on as the enrolled user without
   opening Tauri. Inspect the Task Scheduler XML, process owner/session and medium
   integrity token. Confirm browser bridge, foreground observations, Pomodoro
   feedback and coverage operate. Start a second agent: it must exit without a
   second listener/guard. Terminate Tauri; enforcement must continue. Log off and
   log on; the owner task must start again. Test a crash and bounded restart, then
   deliberate sustained absence: UI/CLI must show missing/degraded coverage while
   commitments and executable fallback remain. A non-owner logon must not enroll
   itself. Multi-session/multi-user enrollment is unsupported.
6. **Every removal entry point.** With a manual protected list and with overlapping
   protected lists, try Windows Settings uninstall, direct `uninstall.exe`, `/S`,
   `/S /DELETE_DATA` and `/UPDATE`. Each denial must return failure before stopping
   the service/agent, removing task/SCM/native registrations, changing the hosts
   section, deleting any file or touching the DB. Compare before/after resource
   snapshots. Query the service decision and verify every blocker and release.
   Test schedule gaps, authorized occurrence unlock, allowance pauses and
   Pomodoro work/pause/break according to the documented legacy semantics.
7. **Unavailable authority and hostile IPC.** Stop/break the authority in an
   unprotected snapshot. All ordinary uninstall modes must fail within the bounded
   helper/transport deadlines, retaining every enforcement resource. Never infer
   permission from an absent pipe. Test forged unlocked/admin/update fields,
   unauthorized images, split tokens, wrong owner, malformed/oversized messages,
   stale endpoint/PID, permit replay/expiry/retention mismatch and a policy change
   between prepare and consume. No destructive callback may run on denial.
8. **Authorized cleanup and retention.** With no current uninstall blocker and a
   healthy owner agent, keep the interface closed and remove once retaining data,
   once deleting authoritative data. Verify service normal stop does not trigger
   failure recovery; only the verified Focuser task and native host keys are removed.
   Confirm agent exit, restored journaled browser values, unrelated later registry
   values preserved, unrelated hosts lines intact, enumerated binaries/shortcut
   removed, and source profile untouched. A policy change at a journaled numbered
   browser slot must fail cleanup with its journal preserved. Restart a browser to
   test cached policy refresh. Retention keeps policy/budgets/history; deletion
   removes DB/WAL/SHM but retains the private recovery tombstone/config/logs.
9. **Interrupted maintenance and partial failures.** Snapshot and inject failures
   after permit consumption, during agent drain, after ready, after SCM stop,
   task removal, native cleanup, service deletion, DB deletion and binary deletion.
   Kill only the exact Focuser helper process used by the test. Before SCM stop,
   enforcement must resume after helper loss/stall, with journal and frozen writes.
   Reboot after consumed authorization: existing commitments and quotas must
   survive and interrupted state must be explicit. After SCM/resource cleanup
   begins, nonzero failure and durable step records are required; no empty DB or
   installer overwrite may silently "repair" the result. Hold a binary open to
   test NSIS deletion failure; it must not report success or schedule a misleading
   reboot-pending success. Use the documented manual administrator recovery path
   for each damaged state and verify resumed commitments before proceeding.
10. **Platform regressions and release acceptance.** Test Tauri as an ordinary user,
    CLI denials, optional UI autostart, native messaging, tray/close behavior,
    frontend errors, data import and settings reset. Confirm resetting/deleting
    user data cannot erase authority identity or maintenance metadata. Verify
    Windows frontend updater install/download commands are denied by capabilities,
    native update action says `upgrade_deferred`, and other platforms retain their
    existing updater capability. Do not release strict-mode claims until real
    UAC/SCM/ACL/task/NSIS and reboot results are attached.
