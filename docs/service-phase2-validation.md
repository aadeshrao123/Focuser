# Service phase 2 implementation and validation

Direct testing update: the user chose this machine without a VM. The first installer
attempt exposed an ACL defect before provisioning. The defect is fixed; its six-file
payload was preserved in an archive. See `service-direct-test.md`. The corrected
package and actual Windows permission probes below supersede the original artifact.

Completed 18 October 2026 in `D:\Focuser`. No `AGENTS.md` was present in the
repository or its D: ancestor. The phase 1 implementation and its validation/VM
procedures were inspected first. Existing unrelated working-tree changes were
preserved. Nothing under `extension/` was changed, installed or rebuilt.

## Implemented changes

| Files | Change |
|---|---|
| `crates/focuser-service/src/{installation,main,storage,lib}.rs`, service `Cargo.toml` | Explicit elevated provisioning/repair; protected existing-configuration checks; private maintenance lock; no empty replacement during repair; automatic LocalSystem SCM registration; bounded 5/10/30-second recovery then NONE; independently registered owner task; machine native host registration; scoped, journaled cleanup |
| `crates/focuser-service/src/{authority,removal}.rs` | All-list removal decisions using existing manual/scheduled/Pomodoro semantics; stable blockers/release information; actual helper/elevated-token checks; 15-second caller/retention/revision-bound single-use permits; serialized consumption and policy writes; durable interrupted state; agent drain and clean exit; service-authorized stop; upgrade explicitly deferred |
| `crates/focuser-service/tests/{authority,removal}.rs` and service unit tests | Policy/overlap/gap/break/restart/denial/permit/order/interruption/repair safeguards |
| `crates/focuser-ipc/src/{protocol,windows}.rs` | Additive v1 maintenance/removal messages and defaulted agent directives; limited BA pipe connection ACL; actual token elevation/group checks; unknown authority claims rejected. Ordinary commands remain owner-only |
| `crates/focuser-session-agent/src/{main,lifecycle,browser_guard,browser_recovery}.rs` | Unprivileged per-session singleton; reconnect cursor; authorized browser/startup cleanup and zero-code exit; coverage reconstruction after abort; foreign browser-policy changes preserved and cleanup refused on conflict |
| `crates/focuser-common/src/types.rs` | Clock-injected protection evaluation sharing the existing semantics, for deterministic service decisions/tests |
| `crates/focuser-core/src/db/mod.rs` | User resets preserve private authority identity/maintenance metadata |
| `crates/focuser-ui/{Cargo.toml,tauri.conf.json,tauri.windows.conf.json}` | Pinned Tauri Rust/build; Windows NSIS/per-machine-only packaging; four required sidecars; no Windows updater artifacts; explicit WebView2 prerequisite |
| `crates/focuser-ui/nsis/{installer.nsi,installer-hooks.nsi}` | Full pinned-contract installer replacing stock lifecycle; reject existing installations and updates before files/old uninstallers; reject ambiguous migration choices; same service authorization barrier for every uninstall mode; enumerate owned files and fail required cleanup errors; old hook file retired |
| `crates/focuser-ui/src/native.rs`, `capabilities/{default,updater-nonwindows}.json`, generated `gen/schemas/capabilities.json` | Windows native updater returns `upgrade_deferred`; direct frontend updater permission removed on Windows; other supported desktop platforms retain updater capability |
| `scripts/{build-windows-installer,audit-windows-installer,test-installer-audit}.ps1`, `.github/workflows/release.yml` | Pinned CLI 2.10.1; protected NSIS-only Windows release pipeline; no portable/MSI/updater ZIP publication; actual rendered-script audit and mutation tests |
| `README.md`, `docs/service-{authority,phase2,vm-test}.md` | Architecture, removal contract, retention, repair, explicit administrator recovery, deferred updates and VM acceptance |

No production CLI/UI fallback database was introduced. Browser commands continue
through the service authority. The installer provisions the agent independently
of optional Tauri startup. Missing coverage remains visible and does not clear
commitments.

## Installer artifact

* `D:\Focuser\target\phase2-installer\Focuser_0.10.0_x64-phase2-acl-fix-setup.exe`
* **Debug, GNU x64, unsigned, direct evaluation at user request. Installation acceptance remains pending.**
* Size: **34,340,285 bytes**.
* SHA-256: `B294F7AAF389228C927F4384D2EB21441F1A3C2A01319EBED77A867F6634CD29`.
* Adjacent `manifest.json`, `installer.nsi`, `build.log`, `audit.log`, and
  `pe-imports.txt` retain provenance/inspection evidence.

The application and all sidecars compiled with Rust 1.99.1 GNU, Tauri 2.10.3 /
build 2.5.6, prebuilt Tauri CLI **2.10.1**, and **NSIS 3.11**. The available local
host is GNU; MSVC release compilation/signing is not claimed. Debug host bundling
uses an explicit GNU target and a real staging directory, avoiding the prebuilt
CLI's default MSVC sidecar lookup. Release CI uses the explicit MSVC target.

The compiler-tool download matched Tauri's pinned checksum. A transient cache
rename failure was resolved by a normal checked rename; no Windows security or
recovery settings were changed. The original package was attempted by the user and failed before service/task
provisioning. Its incomplete payload was archived with no data deletion. The
corrected installer is for the user's requested direct acceptance testing.

## Checks performed

* **414 Rust unit/integration tests passed**, combining the unchanged common/core/
  app/CLI suites with final IPC/service/agent suites; **2 common doctests passed**.
* Final focused suites: IPC 6; service unit 43; authority integration 7; removal
  integration 4; real development Windows-pipe integration 3; session agent 17.
* Relevant common/core/app/CLI/native/IPC/service/agent Clippy `--all-targets --
  -D warnings` passed; new IPC/service/agent changes were checked again after edits.
* **278 frontend tests passed**. The existing ignored slow-machine test harness
  uses one worker, 60-second test deadlines and 30-second asynchronous waits; test
  assertions and application behavior were not relaxed.
* Frontend TypeScript checking and production Vite build passed. Vite's existing
  large-chunk warning remains. Tauri's pre-existing `.app` identifier warning is
  unchanged.
* The compiled Tauri `--export-bindings` path regenerated the **same TypeScript
  contract**. Only checkout line endings differed; those generated line-ending
  changes were restored. Meaningful generated capability changes were retained.
* The cross-process scratch-authority harness passed shared state, identical
  protected denials after separate CLI lifetimes, and restart persistence. Its
  foreground authority always had machine enforcement disabled and was stopped.
* Windows UI and all four clients/service/agent built; NSIS successfully compiled
  the actual rendered installer. It was never executed.
* The actual rendered script was inspected, including `.onInit`, all pre-copy
  checks, WebView prerequisite, install actions, `un.onInit`, retention and every
  uninstall action. Authenticated helper failure aborts before installer cleanup;
  no stock app-killing, automatic old-uninstaller/update or per-user path remains.
* Eight audit mutation cases were rejected: unresolved template placeholders,
  cleanup before authorization, missing denial abort, ambiguous migration choice,
  per-user elevation, broken child inheritance, recursive container grants, and a
  missing GNU WebView loader. The actual rendering
  exposed and prompted correction
  of a Handlebars backslash escape in shortcut/deletion paths; the final script
  contains the concrete expected paths for all five binaries.
* The corrected rendered permission commands passed a real Windows scratch-file
  test for executables and nested resources: files remain readable, inherit the
  protected root and grant Users no write/delete/ACL rights. PowerShell parsing
  passed for every changed script. No Rust/frontend source changed for this fix.
* Static PE imports were inspected for all five binaries and the WebView loader; the import record is
  retained with the artifact. No separate MinGW runtime DLL was required. The GNU UI's imported
  `WebView2Loader.dll` is included through Tauri's automatic resource map and the
  custom template's resource install/cleanup loops; the host staging script copies
  it alongside the UI before bundling. WebView2 Runtime remains a separate explicit
  prerequisite.
* A redundant final debug test build exhausted D: during its executable link.
  The four freshly compiled service test binaries were preserved, stripped only
  of debug symbols and executed directly: 43 unit, 7 authority, 4 removal and 3
  real Windows-pipe tests passed again (`target/phase2-final-direct-tests.log`).
  The final service Clippy and complete installer builds passed independently.
* Final Git checks confirm no extension changes; unrelated phase 1 changes remain.

Evidence is also retained under `target/phase2-*.log` / frontend JSON reports.
These checks exercise policy and failure ordering without provisioning production
resources. They do not establish real SCM/UAC/Task Scheduler/ACL acceptance.

## Remaining limitations / explicitly deferred checks

* Real service install, Program Files/ProgramData/task/SCM ACL behavior, standard
  owner plus separate UAC administrator credentials, endpoint validation against
  a real SYSTEM service, logoff/logon startup, UI termination with real strict
  enforcement, silent uninstall, data retention/deletion, failure recovery and
  reboot are **unverified pending direct acceptance using `service-direct-test.md`**.
* Installer execution, destructive cleanup, and injected power-loss/partial
  maintenance failures were not run on the development machine. Follow the phase
  2 section of `service-vm-test.md`, including all interruption checkpoints.
* One enrolled SID and one concurrent interactive session remain supported.
  Ordinary removal needs a healthy enrolled agent to restore user-session browser
  resources; missing/failed coverage fails removal rather than assuming cleanup.
* Browser policy restoration may require a browser restart to refresh its cache.
  Concurrent external writers can cause a conservative cleanup failure; journals
  are retained for recovery.
* Existing packages, retained-data re-enrollment, same-version reinstall and all
  binary upgrades are deferred through the installer. Repair uses the same
  installed trusted binaries and preserves data. Damaged/missing binaries, bad
  configuration and interrupted removal require explicit administrator recovery.
* WebView2 must already be installed. Production signing and MSVC release/Windows
  acceptance remain required before field deployment.
* There is no administrator-proof claim, shell-killing heuristic, hidden update
  bypass, automatic emergency removal, or Windows recovery modification.

The next implementation phase is signed, service-authorized updates with protected
staging, installation/package identity, transaction recovery and rollback across
power loss. Tauri's Windows updater must remain deferred until that exists.
