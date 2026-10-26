# Phase 1 implementation and validation — 2026-10-17

The Windows UI no longer owns an engine, database, enforcement loop or browser
HTTP listener. Production UI, CLI and native messaging use the service authority.
The independent session agent preserves interactive observations, browser
installation recovery, sounds and notifications. No extension files changed.

## Main changed files

| Files | Change |
| --- | --- |
| `Cargo.toml`, `Cargo.lock` | Three workspace crates, shared IPC dependency, SQLite backup support |
| `crates/focuser-ipc/{Cargo.toml,src/lib.rs,src/protocol.rs,src/windows.rs,src/paths.rs}` | Versioned frames, deadlines/limits, actual Windows client-token authentication, SCM/pipe-owner endpoint authentication, separate pinned development endpoints and safe scratch paths |
| `crates/focuser-service/{Cargo.toml,src/main.rs,src/lib.rs,src/authority.rs,src/server.rs}` | SCM/foreground modes, independent policy clock, one app context, protected command execution, executable/path enforcement, service-side interactive-observation decisions |
| `crates/focuser-service/src/{storage.rs,hosts.rs,browser_api.rs}` | Private ProgramData provisioning, exclusive lock, transactional explicit backup/migration, hosts fallback and restricted browser handlers |
| `crates/focuser-service/tests/{authority.rs,windows_pipe.rs}` | Persistence, protected denials, accounting, independent transitions, real pipe identities/framing/delivery/overload/disconnects |
| `crates/focuser-session-agent/{Cargo.toml,src/main.rs,src/api.rs,src/observation.rs}` | Unprivileged HTTP compatibility bridge, Windows TCP-owner checks, bounded parsing, independent polling/observations and feedback |
| `crates/focuser-session-agent/src/{browser_guard.rs,browser_recovery.rs,native.rs,audio.rs}` | Extracted interactive browser recovery, launching and audio; no authority database |
| `crates/focuser-app/src/{execute.rs,context.rs}`, `crates/focuser-core/src/allowance.rs` | Atomic autostart weakening check, bounded UI event queue, separate strict/loose service accounting streams |
| `crates/focuser-cli/{Cargo.toml,src/main.rs,src/args.rs,tests/cli.rs}` | Production service client, stable denials/exit codes, explicit non-enforcing scratch mode |
| `crates/focuser-native/{Cargo.toml,src/main.rs,src/ipc_client.rs}` | Service-backed native bridge and honest disconnected ping |
| `crates/focuser-ui/{Cargo.toml,src/main.rs,src/windows_main.rs,src/windows_autostart.rs,src/typed_commands.rs,src/native.rs,src/sound.rs}` | Windows client lifecycle; local user-privilege icons/dialogs/preview; development window/startup/update isolation |
| `crates/focuser-ui/src/legacy_main.rs` | Preserved non-Windows desktop lifecycle and previous recovery work |
| `crates/focuser-ui/frontend/src/{lib/commands.ts,components/blocking-health-banner.tsx,components/blocking-health-banner.test.tsx}`, `frontend/messages/*.json` | Disconnected/missing-agent states and localized coverage messages |
| `crates/focuser-devserver/{Cargo.toml,src/main.rs}` | Scratch path validation and refusal to use the production browser port |
| `scripts/test-authority-dev.ps1`, `README.md`, `docs/service-*.md` | Nonpersistent cross-process harness, architecture, migration and disposable-VM procedure |

The previously changed extension-profile monitor and legacy UI browser-recovery
files were preserved. `bindings.ts` and the Rust command/result schemas remain
unchanged; existing generated frontend types still type-check. Build-generated
schema-only changes were restored.

## Performed checks

- 394 Rust unit/integration tests passed across common, core, app, IPC, service,
  CLI and session agent. This includes three real Windows named-pipe tests and
  the extracted browser-guard regressions. Two Rust documentation tests passed.
- `cargo clippy --all-targets -- -D warnings` passed for IPC, service, session
  agent, CLI, native, app and core. The final service changes were checked again.
- Windows source checks passed for service, session agent, native, CLI, devserver
  and Tauri; `cargo check -p focuser-ui --all-targets` also passed. The checkout
  lacks packaged sidecars, so source checks used the process-local override
  `TAURI_CONFIG={"bundle":{"externalBin":[]}}`; packaging/release installation
  was not claimed or changed.
- Frontend type checking and Biome checks for changed TS/TSX files passed.
- All 278 frontend tests passed. Default 5-second Testing Library waits and
  20-second test deadlines intermittently failed in the unchanged schedule UI
  suite on this runner; the isolated 40-test schedule suite passed. The complete
  passing run used one worker, a temporary ignored setup with
  `configure({asyncUtilTimeout:30000})`, and a 60-second test deadline. No test
  expectations or product code were changed to make these assertions pass.
  The report is `target/phase1-frontend-slow.json`; the temporary config lives
  under `crates/focuser-ui/frontend/node_modules/.cache/`.
- The foreground PowerShell harness passed with enforcement disabled: separate
  real CLI processes share one authority, receive protected deletion denials,
  and preserve the same commitment after terminating/restarting that authority.
  Scratch logs/databases are retained under `%TEMP%\Focuser-Authority-Test-*`.
- The production CLI, with no installed authority, returned `service_unavailable`
  and explicitly reported that no local database fallback was started.
- Migration checks covered live WAL contents, all tables, commitments, daily
  usage, shared-budget anchors/intervals, Pomodoro state/history, unknown tables,
  corrupt-source failure and refusal to replace an existing destination.
- `git diff --check` passed; `git diff -- extension` is empty.

The cross-process harness caught a Windows delivery race: disconnecting a pipe
can discard unread replies. The protocol now includes a bounded acknowledgement
and a delayed-reader regression. Overload and early-disconnect tests also verify
that IPC failures do not terminate the independent policy clock.

## Unverified OS acceptance and limitations

No persistent service, startup task, strict enforcement or live migration was
installed/activated on the everyday host. No disposable VM was used. Actual SCM
installation/boot recovery, production ACL enforcement against another Windows
user, real hosts/process enforcement, live Tauri termination under protection,
real browser recovery and reboot continuity remain **unverified**. Follow
[service-vm-test.md](service-vm-test.md) before production deployment.

The current design supports one policy-owner SID and one browser bridge.
Window-title coverage, foreground usage and desktop feedback require the
independent login agent; missing coverage is explicit, with affected browser and
strict-app access failing closed. Multi-user/RDP transport, installer integration,
uninstall gating, advanced tamper controls and update authorization are later
work. Agent desktop notices currently use English. Administrators retain control
of Windows; administrator-proof protection is not claimed.

Next: complete disposable-VM acceptance, then integrate secure service
provisioning/migration and independent agent login registration into the
installer before adding uninstall and update authorization gates.
