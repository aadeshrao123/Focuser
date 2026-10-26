# Phase 3 implementation and validation

Completed 18 October 2026 in `D:\Focuser`. No `AGENTS.md` was present in the
repository or its D: ancestor. The phase-1/2 validation, VM procedure and actual
service/installer implementation were inspected. Existing unrelated work was
preserved. Nothing under `extension/` was changed or rebuilt.

No installer, persistent service, startup task or production policy was activated
for phase 3 on this machine. Earlier phase-2 direct testing is documented separately.

## Changed files

| Files | Phase-3 change |
|---|---|
| `crates/focuser-service/src/update.rs` | Signed manifest contract, fixed HTTPS feed, protected bounded staging, durable transaction, cancellation/worker serialization, policy binding, security floor and cache limits |
| `crates/focuser-service/src/update_broker.rs` | Pinned SYSTEM broker, Authenticode certificate validation, payload handle locks, checkpoint, known-good binary replacement/rollback, startup guards and independent boot recovery task |
| Service `Cargo.toml`, `Cargo.lock` | Locked existing Minisign verifier; SHA-256, semver, bounded HTTP/Rustls support and Windows trust APIs |
| Service `authority.rs`, `main.rs`, `installation.rs`, `lib.rs` | Service-owned update API, persistent policy revision/browser grace, conservative activation checks, serialized maintenance, broker consumption, pre/post-open checkpoint guards, recovery task provisioning/authorized cleanup |
| `crates/focuser-ipc/src/protocol.rs`, `windows.rs` | Additive v1 update requests/status, actual SYSTEM broker identity checks, cooperative client shutdown, unknown authority claims rejected |
| `crates/focuser-session-agent/src/main.rs`, `browser_guard.rs` | Update exit preserves browser journals; service-owned grace survives restarts; authoritative inactive coverage clears grace without tightening legacy schedule behavior |
| `crates/focuser-cli/src/args.rs`, `main.rs`; `crates/focuser-native/src/main.rs` | CLI status/check/stage/activate/cancel; native bridge exits its own process for authenticated replacement |
| UI `src/native.rs`, `windows_main.rs`, `legacy_main.rs` | Windows updater routes to authority; cooperative interface exit; non-Windows updater retained with separately gated command definitions |
| Frontend `lib/native.ts`, `lib/updates.ts`, `components/update-check.tsx`, `update-pill.tsx`, ten message catalogs | Shared authority polling, progress/reasons, stage/activate/cancel and current sidebar availability |
| `crates/focuser-core/src/db/migrations.rs` | All schema transformations/version records commit transactionally; failed migration rolls back |
| `crates/focuser-ui/nsis/installer.nsi` | `/REQUEST_UPDATE` extracts only a bundled IPC requester into newly created private BA/SYSTEM objects, requests live authority, cleans its requester and exits before installed-file extraction. Raw installer replacement remains refused. Uninstall authorization barrier retained |
| `scripts/build-authority-update.ps1` | Release-only signed manifest tooling; requires separately Authenticode-signed payloads and an external signing key |
| Installer build/audit scripts, `.github/workflows/release.yml` | Actual generated-script checks, private-requester/fallthrough mutations, public publisher-pin input, NSIS-only Windows output retained |
| Service/IPC integration and unit tests, migration failure test, frontend update tests | Signature/substitution, replay/expiry/revision/floor, failure/recovery boundaries, data/grace preservation, uninstall regression, client shutdown, actual Windows file sharing and unsigned-code rejection |
| Frontend `routes/scheduled-protection.test.tsx`, locale fixture | Wait for rendered text before expensive grid accessibility scans, then retain the original button role/protection assertions; update-message reason fixtures |
| `README.md`, `docs/service-phase3.md`, this report, `service-phase3-acceptance.md` | Supported contract, continuity gap, release requirements, recovery and remaining acceptance |

## Checks performed

| Check | Result |
|---|---|
| Rust tests: common, core, app, IPC, service, agent, CLI, native; all targets | **435 passed**, zero failures; `target/phase3-tests-final.log` |
| Strict Clippy for the same eight crates/all targets, `-D warnings` | Passed; `target/phase3-clippy-final.log` |
| Frontend Vitest, threads pool, two workers | **284 passed**, zero failures; `target/phase3-frontend-verified.json` |
| Frontend TypeScript and Vite production build | Passed; existing large-chunk warning; `target/phase3-frontend-build-final.log` |
| Generated Rust-to-TypeScript command bindings export | Matched tracked content; original line endings restored; `target/phase3-bindings.log` |
| Windows GNU sidecar and Tauri builds | Passed; `target/phase3-sidecar-final.log`, `target/phase3-ui-final.log` |
| Final NSIS build | Passed; `target/phase3-installer-final.log` |
| Actual rendered NSIS authorization/order/private-object/payload audit | Passed; ten deliberately unsafe installer mutations rejected |
| Rendered installer permission commands on scratch files and a nested resource | Passed with real Windows ACL/read checks; no installation folders were changed |
| Unsigned release-package negative test | Rejected before manifest signing/output; no private release key used |
| PowerShell script parsing, formatting and Git whitespace checks | Passed; no extension diff |

The early default-forks frontend run had schedule query timeouts and a worker
termination timeout. A focused rerun reproduced starvation from repeatedly scanning
the 168-cell grid before query data rendered. The fixture now waits cheaply for the
control text and retains its accessibility/disabled/protection assertions. The final
full suite passed with the threads pool; no schedule product behavior was changed.

Tests include real isolated Minisign signatures (public fixtures only), wrong
product/version/schema/expiry, unsigned/tampered payloads, signed-manifest cache
substitution, bounded/truncated/disk-full writes, durable reload and consumed-state
recovery, rollback/floor incompatibility, failed migration atomicity, persistent
commitments/budgets/grace/revision, protected UI/CLI denials and uninstall cleanup
ordering. Failure-injecting broker Ops test the state machine; they do not prove
SCM/file replacement behavior through an actual installation or reboot.

## Artifact

- `D:\Focuser\target\phase3-installer\Focuser_0.10.0_x64-phase3-debug-setup.exe`
- 36,972,919 bytes; Windows x64 **unsigned GNU debug** package.
- SHA-256: `5D331900AD75E473DD769606F903FD8C983F6D9D333A5E0674DC48179543D20C`
- Adjacent `SHA256SUMS.txt`, `build-manifest.json`, `installer.generated.nsi` and
  `installer-audit.log` retain the exact payload hashes and rendered-script evidence.
- Pinned Tauri Rust 2.10.3, build 2.5.6, CLI 2.10.1; cached NSIS 3.11.
- NSIS is the only Windows package; no MSI or generic Windows updater archive was
  produced. Five executables plus GNU WebView2Loader.dll are packaged.
- The final script was inspected after rendering, including requester ACL/file
  creation, the early update exit, early fresh-install barrier and the uninstall
  helper failure/Abort path preceding every installed-file deletion.

## Signing and limits

Release credentials are unavailable. No production signed update/feed was created
or published. `FOCUSER_AUTHENTICODE_SHA256` is absent in this development build;
production update checks fail with `signing_unconfigured`. There is no unsigned
production bypass. Update-manifest Minisign signing and Windows executable/installer
Authenticode signing are separate requirements.

Supported updates are stable x64, IPC-v1, schema-8-neutral packages only. The baseline
broker/trust anchor remains pinned; trust rotation and schema-changing updates need
a later coordinated release. An older unsigned phase-2 installation cannot silently
bootstrap into this build; ordinary reinstall and raw `/UPDATE` remain deferred.

Old enforcement runs during download/staging. After consumed checkpoint, mutable
accounting and executable/path enforcement pause through replacement/validation;
hosts/browser restrictions remain but do not provide equivalent coverage. Activation
therefore defers for strict protection, scheduled-protection configuration including
gaps, retained locks, strict allowances and active/paused Pomodoro. Existing list
release/removal semantics are not strengthened. The UI can be reopened after
maintenance to inspect durable status.

No old policy database is restored. Failed/incompatible rollback produces an
explicit recovery fault with bounded attempts, not an empty writable authority.
Agent absence does not authorize release and does not prevent broker recovery;
interactive coverage remains degraded until the independent owner agent returns.
The existing single-owner/session limitation remains.

## Explicitly unverified acceptance and next task

SCM installation/start/stop and failure recovery, UAC/alternate credentials, actual
SYSTEM task execution and reboot ordering, standard-account attacks on installed
ACLs/reparse paths, the NSIS private requester Win32 security-attribute path, signed
certificate acceptance/revocation behavior, real multi-file replacement/held-open
binaries, disk-full/power-loss/reboot recovery, and successful/denied installed
interactive/direct/silent removal were **not exercised**. Only scratch Windows
ACL/file-sharing, unsigned WinVerifyTrust rejection and development-pipe checks were
run. MSVC release, Linux/macOS compilation and real signed release acceptance also
remain unverified.

Use `docs/service-phase3-acceptance.md` for deliberate future operator testing; a VM
is not a prerequisite for this implementation and none was used. Do not run those
installation actions automatically on this development machine.

Next task: configure controlled release signing, implement the explicit trusted
legacy-bootstrap/trust-rotation path, then perform the documented Windows acceptance
and failure-injection procedure before enabling production update publication.
