# Experimental kernel protection validation — October 19, 2026 (Hawaii)

## Outcome

An experimental driver/service implementation and policy test harness have been
added. **Administrator-resistant deletion protection is not complete, installed
or enabled.** No production driver or replacement installer was produced. Do not
interpret passing user-mode tests as a validated Windows protection driver.

The release blockers and isolated acceptance procedure are in
[`drivers/focuser-guard/README.md`](../drivers/focuser-guard/README.md).

## Changed implementation

- `drivers/focuser-guard/shared`: fixed-width versioned protocol and the policy
  state machine shared by the kernel adapter and portable executable tests.
- `drivers/focuser-guard/kernel`: experimental minifilter, real-token/service-SID
  connection checks, process/thread handle callbacks, registry callbacks and
  authorized unload logic. Native roots are limited to Focuser's fixed default
  locations; the loaded driver discovers its own image through the driver object.
- `drivers/focuser-guard/FocuserGuard.vcxproj` and `.inf.in`: WDK project and a
  deliberately unresolved installation template. No installable INF is emitted.
- `crates/focuser-service/src/kernel_guard.rs`: bounded FltMgr transport, strict
  response validation, stored-commitment deadline construction and diagnostics.
- Service authority/lifecycle integration: fence before policy writes, reload
  stored state before clearing a fence, preserve enforcement from the cached
  policy if kernel communication fails, and reuse the existing removal permit.
  Driver-unaware updates and live provisioning are deferred in this profile.
- Four PowerShell scripts: portable tests, production/toolchain gate tests,
  explicit experimental WDK build and creation of audit reference policy XML.

Ordinary builds have no enabled `kernel-file-guard` feature and do not connect to
or claim kernel protection. Rust-to-TypeScript command shapes and browser
extension source were not changed. Unrelated existing workspace changes remain.

## Automated results

1. `cargo test --locked --release -p focuser-service -j1`:
   **103 tests passed** in the normal profile regression run. This covered
   service-owned denials, expiry/unlock, overlapping commitments, breaks, restart,
   UI-client termination, migration, update recovery and local Windows pipes.
2. `cargo test --locked --release -p focuser-service --features kernel-file-guard --lib --bin focuser-service -j1`:
   **63 tests passed** on the final experimental profile. This includes actual
   Windows transport compilation, bounded queue/timeout behavior, malformed
   replies, no replacement transport after disconnection, stored deadlines,
   before-write fencing, lost synchronization and the subsequent stored-state
   reload/storage-corruption regression. The binary test target also compiled.
   These are overlapping suites, not 166 distinct tests.
3. `scripts/test-kernel-guard.ps1`: GCC compiled the shared policy C sources with
   C11, optimization, pedantic warnings and warnings-as-errors. All **6 scenario
   groups** passed. They cover framing/ABI, identities, replay, revision races,
   timed/indefinite commitments, 100ns expiry conversion, controller reconnect,
   permit mismatch/expiry/replay, interrupted changes/removal and path boundaries.
4. `scripts/test-kernel-guard-gates.ps1`: both negative gates passed. Production
   and absent-toolchain attempts fail before creating output or installing.
5. Targeted Rust formatting, PowerShell AST parsing and WDK project XML parsing
   passed. There is no claim that parsing XML compiled the kernel adapter.
6. Windows ConfigCI successfully generated an audit reference policy from the
   staged Focuser binaries. The XML was inspected: UMCI and Audit Mode are enabled,
   it remains unsigned, and it contains 16 allow rules. It is explicitly not a
   complete OS allowlist. No policy binary was compiled, signed or deployed.
   The scan warned that an unelevated process could not create a shadow copy;
   scanning the reference files subsequently completed successfully.

Artifacts:

- `C:\Users\Ahmed Ameen\AppData\Local\FocuserDevTools\kernel-guard-tests\guard-policy-tests.exe`
- `C:\Users\Ahmed Ameen\AppData\Local\FocuserDevTools\kernel-guard-tests\AppControlAudit\Focuser-REFERENCE-AUDIT.xml`

Audit XML SHA256:
`6FC08AD8A9099A957EBE9CAEBDE527FCBF6629CED10F8E8C96A37E23CFB1F2FE`

## Explicitly unverified / unfinished

- No MSVC installation is registered and the required WDK headers/build toolchain
  are absent. No `.sys` was compiled, linked, statically analyzed or verified.
- No signing certificate/Microsoft driver signing, production altitude enrollment,
  signed App Control policy or external key custody is configured.
- Full file-identity/context coverage (including aliases, hardlinks, existing
  handles/mappings and name-query failures), service/process startup races,
  durable boot/clock recovery and driver-aware installation/update/removal remain
  release blockers. This source is not a complete security boundary against an
  administrator.
- No driver load/unload, elevated cleanup refusal, real protected file deletion,
  reboot, power-loss or Driver Verifier acceptance was performed. Neither VM nor
  physical test-machine acceptance was run.

The next engineering work is WDK compilation and closing those kernel/lifecycle
gaps, followed by isolated acceptance and signing. It is not enabling the current
prototype on this everyday computer.

## Machine/data preservation

No Focuser service or guard driver was installed or started. No enforcement,
account changes, Secure Boot changes, recovery restrictions or certificate trust
changes were activated. No browser extension code was modified. Existing retained
authority data and recovery backups were not imported, reset or changed.

The machine remains in its previously diagnosed damaged/uninstalled state; this
development work does not silently repair or reactivate that installation.

To make room on nearly full D:, only the known superseded
`D:\Focuser\Focuser-Setup-FIXED.exe` artifact was moved to the existing recovery
tools directory on C:. Its SHA256 was checked before/after. The current installer
was left in place. Small missing GCC runtime/manifest build dependencies were
extracted from official MSYS2 packages into the existing user-local toolchain;
they are not product payloads or Windows services.
