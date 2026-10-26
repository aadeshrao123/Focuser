# Focuser Guard — experimental, NOT deployable protection

This directory is a development implementation, not an administrator-proof
product. **No production `.sys`, signed catalog, installable INF or enforced
App Control policy is shipped.** Ordinary Focuser packages do not include or
activate it. The existing unsigned installed build cannot bootstrap trust into
this profile. The browser extension is unchanged.

Normal service builds now also have a separate user-mode sharing-lock layer;
see [its exact scope and limits](../../docs/file-deletion-protection.md). It can
deny ordinary elevated file deletion while the service remains alive. It does
not close this prototype's signing, reboot, process-tampering or kernel coverage
gaps and must not be marketed as the completed administrator-resistant design.

## Implemented source

- A shared C state machine and fixed 80-byte request/56-byte reply protocol.
  Version, length, installation identity, sequence, policy revision, reserved
  fields and one-use removal tokens are checked. There are no supplied paths,
  claimed identities, unlocked flags, general commands or shell execution.
- The minifilter adapter intercepts destructive opens, writes, delete-on-close,
  disposition changes, rename/link destinations, security changes and filesystem
  controls on three enrolled native NT roots and its own loaded driver image.
  That image path is discovered from the actual driver object and normalized;
  it is not supplied by a client. Parent rename/delete checks prevent
  a simple ancestor-path bypass. Other applications' files are not target roots.
  Only the fixed x64 default Focuser/ProgramData/task suffixes are accepted; custom
  installation layouts are unsupported, not arbitrary caller-selected targets.
- A kernel connection checks the actual primary token for LocalSystem and the
  **FocuserAuthority service SID**, requires Session 0 and the enrolled executable
  path, and accepts one connection. The default FltMgr port descriptor is only
  an initial access check; it is not sufficient controller authentication itself.
- Registry callbacks target the two Focuser service keys and Focuser's task tree.
  Object callbacks strip dangerous new/duplicated process/thread handle rights to
  the connected authority while protection is active. Kernel callers and unrelated
  processes are not blocked.
- Service integration serializes driver transitions with the existing maintenance
  mutex. Writes are fenced before the durable policy revision changes; after
  success or an application error, the service sends its actual stored decision.
  Losing the driver leaves subsequent mutations blocked in that experimental
  profile, not a replacement database or engine. A read-only
  `_authority.kernel_guard` setting reports disabled/experimental/disconnected.
- Removal still uses the existing caller-bound, revision-bound service permit.
  Only after consumption and a fresh service decision does it prepare/consume a
  separate short-lived kernel removal token. New policy writes cannot race it.
  Existing binary-update activation is explicitly deferred in this profile.
- A bounded one-worker Windows FltMgr transport returns diagnostics after three
  seconds. A stuck kernel call does not create replacement workers or grant
  authority. Closing a client port does not clear the kernel policy.

## Commitments and lifecycle

The service derives protection from **the same removal decision** as ordinary
uninstall. It does not strengthen migrated flags or invent longer commitments.
Pauses and Pomodoro breaks are not unlocks. Overlapping commitments use the
latest relevant original expiry; until-unlocked commitments retain that condition.

Within one boot, a committed UTC expiry is converted to a monotonic deadline.
The wire clocks use Windows' 100ns units. Stored expiry is rounded up to that
precision so conversion cannot release before the original stored instant.
Repeated identical snapshots and controller reconnects do not refill it. Timed
protection expires without requiring a living UI/service. Policy-write intent is
separate: an interrupted transition remains fenced until the authenticated service
restores authoritative state. Initial boot state is unknown/fenced, not empty.
The bootstrap service connects before opening writable storage, then reconciles
the original stored commitment and enrollment. Failed restoration stays pending.
This is not a durable driver journal or a secure-clock implementation.

## Build and portable tests

`scripts/test-kernel-guard.ps1` compiles **the same C policy core used by the
driver** as a user-mode executable. It never loads a driver or changes production
data, accounts, browser policies, SCM or Secure Boot.

An experimental WDK build requires MSVC, the matching Windows SDK/WDK and an
allocated altitude. In a configured WDK build environment:

```powershell
.\scripts\build-kernel-guard.ps1 -AllocatedAltitude <allocated-value> -OutputDirectory C:\FocuserBuild\guard
```

The output is deliberately unsigned and has no installable package. `-Production`
fails explicitly. The `.inf.in` is a review template with unresolved enrollment,
category and signing inputs, not a supported installer.

The Rust profile is `focuser-service/kernel-file-guard`. Only that service profile
tries to connect during production SCM startup. It requires the existing compiled
publisher pin and verifies its own Authenticode identity before connecting. It
cannot provision/repair an installation. The ordinary feature-disabled service
continues its existing behavior and reports kernel protection **disabled**.

`scripts/new-appcontrol-audit-policy.ps1` creates only an audit reference XML from
a reviewed application directory. It does not generate a complete OS allowlist,
sign/enforce/deploy a policy, alter boot options or change Secure Boot. A full
system policy, signed externally and tested with Secure Boot, remains separate
deployment work. Signing is not replaced by an arbitrary runtime client flag.

## Release blockers — must not be represented as implemented guarantees

1. **Kernel code has not been compiled with a WDK or run under Driver Verifier.**
   Neither a WDK nor MSVC is present on this development machine. Portable C and
   Rust tests do not establish kernel API/IRQL correctness or crash safety.
2. Path-only interception is incomplete: pre-existing hardlinks, file IDs,
   pre-existing handles/mappings, paging/name-query failures, reparse points,
   volume aliases/raw-volume access and all rename/FSCTL variants need a complete identity/context
   design. The adapter deliberately avoids denying unrelated global I/O when a
   name cannot be resolved. That permits gaps and **blocks release**.
3. Process protection begins after controller connection. Previously granted
   handles, initial service injection races, broker processes, handle inheritance
   and other privileged helpers are not comprehensively covered. A process name
   or path alone is not proof against an administrator; tested signed App Control
   policy and verified image enrollment are required.
4. No durable kernel journal, protected boot enrollment, secure-clock/rollback
   protocol, driver-aware power-loss transaction or complete task-cache GUID
   coverage exists yet. Boot fences alone do not solve those issues.
   SCM boot writes and service-SID startup must be checked against the registry
   callbacks so a fence does not prevent the genuine service from recovering.
5. Full signed installation/update/removal coordination remains deferred. The
   existing helper does not stop/delete a guard driver package; accordingly the
   experimental service cannot provision one. A loaded guard's unload callback
   requires a consumed kernel removal token. Both ordinary driver-stop requests
   and unauthorized detach must be tested. Driver updates must preserve policy
   and storage rather than use the old binary broker unchanged.
6. Microsoft driver signing, an allocated altitude, signed policy key custody,
   a tested recovery procedure and isolated Windows acceptance are unavailable.
   A signing/acceptance checklist is not cryptographic authorization. No production
   claim may be enabled by setting an environment-variable "certified" switch.

## Required isolated acceptance

Use a disposable test computer or VM; do not start with the everyday development
machine. Retain working recovery media and data backups. Do not disable Windows
recovery or code-integrity safeguards to deploy this prototype.

- Compile x64 with WDK warnings as errors; run static driver analysis, InfVerif,
  Driver Verifier and interoperability tests with antivirus/other filters.
- Prove rejection of a normal user, elevated administrator, unrelated LocalSystem
  service, wrong service SID/image, impersonation and duplicated port handles.
- Exercise ordinary, POSIX and delete-on-close deletion; rename replacement,
  directory/ancestor moves; hardlinks, ADS, mapped files, paging and volume aliases.
- Test active, idle, schedule gaps, pauses, breaks, overlapping deadlines, expiry
  and verified unlock. Idle ordinary uninstall must remain straightforward.
- Kill/close the UI and terminate/restart the service. Test previous process
  handles and controller reconnect; deadlines must not refill or silently release.
- Race policy changes, concurrent cleanup, permit consume/replay/expiry, driver
  unload/detach, SCM/registry/task edits and interrupted maintenance.
- Reboot and simulate power loss at each journal/enrollment/maintenance boundary;
  recover the original DB and expiry. Test disk-full, corrupted storage and a
  missing driver/service without creating an unrestricted replacement.
- Establish an App Control audit baseline first; verify all OS, development,
  accessibility and recovery workflows. Test signed enforcement and rollback only
  after compatibility is proven, keeping signing/recovery keys separately managed.

Sources: [minifilter I/O](https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/about-file-system-filter-drivers),
[deletion variants](https://learn.microsoft.com/en-us/samples/microsoft/windows-driver-samples/delete-file-system-minifilter-driver/),
[FltMgr communication](https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/communication-between-user-mode-and-kernel-mode),
[unload behavior](https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/when-the-filterunloadcallback-routine-is-called),
[driver signing](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-),
[own image discovery](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/nf-ntddk-ioqueryfulldriverpath),
[signed App Control policies](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/deployment/use-signed-policies-to-protect-appcontrol-against-tampering).
