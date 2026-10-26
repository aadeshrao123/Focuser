# Windows release candidate validation — 18 October 2026

## Follow-up: misleading executable configuration error

The follow-up changed only service `storage.rs`/`setup.rs`, their regression tests,
and recovery documentation. Relative, missing, unreadable and non-file paths now
have distinct diagnostics including the failing path. An absent enrolled service
payload returns `repair_payload_missing` and preserves retained policy instead of
suggesting that the user change a SID or initialize new data.

Read-only host inspection found no `C:\Program Files\Focuser` directory and found
an inaccessible-to-the-development-account `C:\ProgramData\FocuserAuthority`
directory. No private policy was read or changed. The archived earlier payload
remains. The damaged installed state still needs explicit administrator recovery
of its original trusted build; this candidate does not bootstrap a different build.

All **94 service tests** passed, including the two new diagnostics/preservation
regressions and existing authority, removal, update and Windows pipe harnesses.
NSIS preprocessing selected exactly the intended service requester. Test log:
`target/installer-path-tests.log`. The original 452-test and frontend results below
remain the earlier baseline; they were not repeated for this Rust-only follow-up.
Strict service Clippy (all targets, warnings denied) and workspace formatting
checks also passed (`target/installer-path-clippy.log`).

The optimized unsigned diagnostic rebuild completed. Actual rendered NSIS audit,
13 unsafe-script mutation checks and scratch file-permission checks passed.
Artifact: `target/windows-repair-diagnostic/Focuser_0.10.0_x64-repair-diagnostic-setup.exe`
(25,862,280 bytes), SHA-256
`6CB0D1D220F73DA213B09F1202299B045B522FEDE375032628CDB2F76C17DCF0`.
Adjacent logs, manifest, rendered script and `verification.json` record the result.
The earlier installer reported by the user was left unchanged. Neither installer
was executed during this follow-up; the damaged live installation is not repaired
and installed Windows acceptance remains unverified.

Later, after the user reported that diagnostic dialog, elevated read-only checks
proved the abandoned database was healthy and empty of policy/history. Explicit
administrator recovery archived the complete directory without changing its
private permissions; all six file hashes matched. The installer was not run.
Thirteen new scratch recovery tests passed, and an argument-only NSIS 3.11 probe
confirmed that the continuation launcher's owner SID and quoted import path are
parsed correctly. Use `target/windows-repair-diagnostic/Continue Setup.cmd` for
the next user-run setup attempt; it prefills an import of that original database
and never chooses empty initialization. See `service-direct-test.md` for the
archive path and precise distinction from ordinary removal/update authorization.

## Earlier release-candidate baseline

Scope: setup/repair, release packaging, and enforcement only while applicable
blocking rules or quotas run. The extension was not edited or rebuilt. No
installer, service registration, startup task, hosts enforcement or browser
policy was activated on this machine.

## Implemented

- `crates/focuser-service/src/setup.rs`: desktop-token owner discovery; exact-code
  repair; conservative archival of unprovisioned payloads after checking independent
  installation resources. Unknown contents, registrations and reparse points fail
  closed. Existing authoritative storage is never replaced with an empty policy.
- Service `installation.rs`, `storage.rs`, `main.rs`: reuse provisioning without
  replacing installed code; readable helper diagnostics; read-only import preflight
  before persistent provisioning; clean-state initialization for a new empty
  installation. Existing profile imports retain their data and commitments.
- `crates/focuser-ui/nsis/installer.nsi`: automatic owner detection; interactive or
  `/REPAIR` retry; private bundled helper extraction; shared registration repair;
  retained existing uninstaller; captured setup errors. Direct replacement/update
  flags and all uninstall authorization barriers remain protected.
- Service `authority.rs`, IPC `protocol.rs`, session-agent `browser_guard.rs`,
  `browser_recovery.rs`, `main.rs`: enabled website rules/quotas govern browser
  coverage; durable service-owned cleanup responsibility; authenticated agent
  restoration acknowledgement; stale snapshots and maintenance cannot authorize
  new browser enforcement. Unknown/unreadable journals never count as confirmed
  restoration. A clean idle installation does not require a live agent for removal.
- Service `hosts.rs`: inactive enforcement does not normalize/rewrite unrelated
  hosts bytes; release removes only the owned section, preserving other entries.
- `scripts/build-windows-installer.ps1`, `sign-windows-file.ps1`,
  `test-release-packaging.ps1`, installer audit scripts and release workflow:
  optimized host packaging; mandatory public release signing; explicit unsigned
  candidate override; certificate/publisher/timestamp checks; artifact manifest;
  automatic owner/private repair installer regressions.
- `README.md`, `windows-release-candidate.md`: current setup, support boundary,
  release instructions and accurate Windows acceptance status.

## Completed automated checks

| Check | Result |
| --- | --- |
| Rust all-target tests across common/core/app/IPC/service/CLI/native/agent | **452 passed**, zero failures (`target/product-tests-complete.log`) |
| Strict Clippy for those eight crates, all targets, warnings denied | Passed (`target/product-clippy-complete.log`) |
| Frontend TypeScript | Passed (`target/product-typecheck.log`) |
| Frontend production build | Passed; existing large-chunk/plugin-timing warnings (`target/product-frontend-build.log`) |
| Release without signing or explicit candidate override | Rejected before build |
| Signing without a certificate; configured nonexistent private certificate | Rejected before signing/build (`target/product-release-gates.log`) |
| Installer template authorization/order and setup-helper checks | Passed |
| Final optimized Windows GNU sidecars and Tauri desktop build | Passed (`target/product-release-final.log`) |
| Actual rendered NSIS script and permission harness | Passed; **13** unsafe installer mutations rejected |
| Six packaged executables/DLLs: PE architecture and runtime imports | All x64; required DLLs accounted for (`verification.json`) |
| PowerShell script parsing; Git whitespace; extension diff | Passed |
| Read-only `setup-owner` probe from the optimized service executable | Passed on this interactive desktop; no provisioning performed |

Focused regressions include empty/app-only/disabled website lists; durable cleanup
acknowledgements and forged client rejection; stale activation during maintenance;
idle removal without an agent versus unknown/pending cleanup; incomplete-payload
archival/retry without data deletion; invalid import preservation; hosts restoration;
original commitment expiry through pauses, both Pomodoro breaks and authority
restart; overlapping commitments and verified unlock; unavailable service and
denied interactive/direct/silent transaction cleanup. Windows pipe/process checks
use isolated development endpoints and scratch databases with enforcement disabled.

## Installer artifact

- `D:\Focuser\target\windows-release-candidate\Focuser_0.10.0_x64-release-candidate-setup.exe`
- **25,866,169 bytes**, optimized `release` profile; signature status **NotSigned**.
- SHA-256: `E1275657262326991A6BFC41DC26D858D0FD6A3A8EF3917C75495B6CDBECF2D9`
- Adjacent `SHA256SUMS.txt`, `build-manifest.json`, `verification.json`, rendered
  `installer.generated.nsi`, build/test/Clippy/frontend/gate logs retain evidence.
- Final staged binaries were checked for the import preflight, maintenance
  enforcement guard and checked restoration path before delivery. The package was
  rebuilt after those final source changes. Previous debug/phase artifacts remain.
- Pinned Tauri CLI 2.10.1, Rust Tauri 2.10.3/build 2.5.6, cached NSIS 3.11.
  Per-machine NSIS is the only produced/supported Windows installer format.
- No installer was executed. Positive installed behavior is not inferred from
  compilation, script audits or PE inspection. WebView2 remains a prerequisite.

## Acceptance boundary

No code-signing certificate is available. Positive signing/timestamp validation
and a signed public release are unverified. No test certificate or unsigned
production update bypass was introduced.

Actual installed UAC alternate-account enrollment, SCM service startup/failure
recovery/reboot, scheduled-agent logon, same-code installer repair, WebView2
prerequisite UX and installed interactive/direct/silent uninstall remain unverified.
VM testing is deferred; these actions were not run on the development machine.
Read-only owner discovery and scratch harness results do not establish installed
acceptance.

Repair requires the exact installed helper bytes and intact authoritative data.
It cannot replace missing/mismatched application binaries, bootstrap an older
installation, clear maintenance journals or override commitments. Existing
administrator recovery remains available. A healthy agent is still required when
owned browser settings need restoration. The existing single-owner/session
limitation remains; there is no claim of protection against a determined admin.
