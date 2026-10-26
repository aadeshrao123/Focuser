# Windows authority, phase 1

Windows production has one authority: `focuser-service service`, registered as
`FocuserAuthority` under LocalSystem. Tauri and the CLI send `focuser-app::Command`
through `focuser-ipc`; neither opens a production database. Closing, crashing or
terminating Tauri does not stop the authority. Non-Windows builds retain the
existing desktop runtime. Nothing in `extension/` is changed.

## Boundaries

| Component | Responsibilities |
| --- | --- |
| Service, Session 0 | SQLite, validation/unlock verification, commitments, schedules, executable/path decisions and termination, hosts fallback, Pomodoro transitions, persisted allowance accounting |
| Independent session agent, interactive owner | Visible window titles and foreground/idle observations, existing 500 ms extension profile monitor and installation-only browser recovery, HTTP browser compatibility bridge, sounds, desktop notices, opening the UI |
| Tauri | Rendering, dialogs, icons and sound preview under user privileges, commands to the service |
| CLI | Commands to the same service; explicit isolated scratch harness |
| Native host | Bounded browser native frames; restricted telemetry/rule requests to the same authority |

Agent desktop notices currently use English text; frontend coverage banners use
the existing localized message catalogues.

The agent must be installed in a protected directory and started **independently
at login**, not as a child whose lifetime is tied to Tauri. This phase provides
the agent executable, not a production installer integration. A missing agent is
reported as degraded. The service keeps evaluating schedules, commitments,
executable/path rules and Pomodoro. Browser access with active web rules/domain
quotas and applications needing strict foreground quotas fail closed after the
8-second observation lease expires. Window-title-only rules cannot be observed
in Session 0; their coverage requires the agent. Sounds/notifications also need
the interactive agent. Neither service startup nor migration opens a browser or
plays a user-selected file as LocalSystem.

This phase supports one configured Windows owner SID and one localhost browser
bridge. Simultaneous different-user browser sessions/RDP require a later
per-session browser transport design. The service only terminates processes
owned by that SID in interactive sessions; PID, image and token are checked
against the same open process handle used for termination. No shell-killing or
uninstaller-process heuristic is added to the service.

## Local protocol and trust

Production pipe: `\\.\pipe\Focuser.Authority.v1`. Development pipe:
`\\.\pipe\Focuser.Dev.<explicit-name>`, with an explicitly pinned server PID.
Frames are little-endian u32 length + UTF-8 JSON, protocol version 1, at most
4 MiB, with 5-second read/write/connect deadlines and at most 16 workers. Each
connection carries one request. After reading a reply, the client sends byte
`0x06` before disconnect; the server bounds this acknowledgement wait to 5 seconds
to avoid dropping unread Windows pipe output. Unknown envelope fields and malformed/oversized
frames are rejected. The server retains a listening instance to prevent a gap
in pipe ownership.

The production DACL grants SYSTEM full control and gives the configured owner
and elevated Administrators limited read/write/synchronize/read-control rights (no create-pipe-instance right), denies
network logon, and uses `PIPE_REJECT_REMOTE_CLIENTS`. After reading a frame the
server impersonates the pipe client synchronously, reads its actual token SID
and session, reverts, and corroborates the kernel-reported process identity.
Ordinary commands still require the owner SID. Phase 2 maintenance also validates
an elevated Administrators token and the protected installed helper image; see
[provisioning and removal](service-phase2.md). There is no JSON role, unlock flag,
claimed SID or UI-challenge-success authority.
Agent/browser and native request classes additionally require the configured
installed executable path and a nonzero interactive session. Installed code and
parent ACLs are checked before production starts. Runtime injection and code
integrity/tamper defenses are later work.

Clients authenticate the endpoint **before sending**: kernel pipe server PID
must equal SCM's running `FocuserAuthority` PID, SCM must configure LocalSystem,
and the actual kernel pipe object owner must be SYSTEM. Service startup checks
its own token is SYSTEM; a standard user does not need permission to open that
SYSTEM process token. Identification-only client SQOS avoids
giving an impostor server an impersonation-capable user token. Development
requires matching the pinned PID, current user SID and pipe owner; it cannot
fall back to the production name. Production never selects development from an
environment variable or a failed connection.

Stable errors include `service_unavailable`, `server_identity`, `unauthorized`,
`version_mismatch`, `message_too_large`, `malformed`, `timeout`, `transport`,
`storage_security`, and existing app errors such as `protected`. Transport
timeouts do not mean a submitted mutation was rolled back; clients must refresh
state before retrying, never replay mutations automatically.

The unprivileged agent binds only `127.0.0.1:17549`. The browser protocol exposes
existing `/api/*` operations, **never `/command`**, service shutdown, settings,
raw SQL or arbitrary administrative commands. It checks the TCP peer's actual
Windows owner/session, rejects ordinary web/null Origins and DNS-rebinding Host
values, bounds headers (16 KiB), bodies (64 KiB), connections (16) and socket
deadlines, and forwards policy operations over authenticated IPC. Extension
Origins/headers are compatibility metadata, not Windows authentication.
Protected browser mutations are rechecked in the authority. Browser ticks cannot
claim application foreground usage. Browser activity remains extension-reported;
cryptographic browser attestation is not claimed by this phase.

## Storage and explicit migration

Use the Windows known-folder API, not a client-provided environment path:
`%ProgramData%\FocuserAuthority\{service.json,authority.db,authority.lock}`.
Owner is Administrators; only SYSTEM and Administrators have access. Startup
rejects unexpected ownership, access grants, null ACLs and reparse points, and
takes an exclusive Windows file lock. SQLite journals inherit the private
directory ACL. An ordinary UI/CLI owner has no direct database access. The
service refuses to start without an already initialized/migrated database.
The configured SID is policy ownership, not file ownership.

Provisioning is explicit and refuses an existing authority directory. Choose
either `provision --initialize-empty` **or** provision without that flag then
`migrate --source <legacy.db>`. Migration uses SQLite online backup (including
WAL/all tables), opens the source read-only, migrates and integrity-checks a
staging copy, syncs it, then publishes with a same-volume create-if-absent hard
link. It never overwrites a destination. Failure leaves the source intact and
does not publish a partial authority. A crash before publication can leave a
private `.migration-*.tmp` file for operator inspection; it is not used at boot.
Do not concurrently use the legacy desktop to modify the source during cutover.
Retain the original DB and any WAL/SHM as a recovery backup until VM acceptance.

Run these elevated **only in the disposable VM**, after securing installed
binaries as described in [the VM procedure](service-vm-test.md):

```powershell
$bin = 'C:\Program Files\Focuser'
$policyOwnerSid = 'S-1-5-21-REPLACE-WITH-VM-USER-SID'
& "$bin\focuser-service.exe" provision --owner-sid $policyOwnerSid `
  --agent-image "$bin\focuser-session-agent.exe" --native-image "$bin\focuser-native.exe"
& "$bin\focuser-service.exe" migrate --source 'C:\VM-Test-Data\legacy.db'
```

`provision`, `migrate` and `service` are separate explicit modes. None installs a
service automatically. SCM STOP is refused while a commitment requires service
continuity. OS shutdown is accepted so reboot/recovery remains available; durable
state is restored on automatic service startup. Administrators can still stop,
delete, replace or reconfigure the service; administrator-proof protection is
not claimed. Installer uninstall gating, update authorization and advanced
tamper controls remain subsequent phases. The old installer/update packaging
must not be treated as deploying this new architecture yet.

## Safe development

Use a new, disposable directory. Foreground development always disables hosts
writes, process termination and browser recovery policies:

```powershell
$scratch = Join-Path $env:TEMP ('Focuser-Dev-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch
# Terminal A, foreground; Ctrl+C stops this nonpersistent authority.
.\target\debug\focuser-service.exe foreground --dev --db "$scratch\dev.db" `
  --pipe-name phase1-test --agent-image 'D:\Focuser\target\debug\focuser-session-agent.exe' `
  --native-image 'D:\Focuser\target\debug\focuser-native.exe'
# Terminal B: replace 12345 with the PID printed by Terminal A.
.\target\debug\focuser-cli.exe --dev --dev-pipe phase1-test --dev-server-pid 12345 list ls
.\target\debug\focuser-ui.exe --dev --dev-pipe phase1-test --dev-server-pid 12345
.\target\debug\focuser-session-agent.exe --dev --dev-pipe phase1-test `
  --dev-server-pid 12345 --http-port 17551
# Standalone, non-enforcing CLI command-core harness (not a production engine):
.\target\debug\focuser-cli.exe --dev --db "$scratch\cli.db" list ls
```

Development UI clients do not join the production single-instance window,
change Windows startup registrations or install application updates.
Development refuses the production authority directory and the legacy live
profile database. Existing hard-link aliases are rejected. The separate
existing React devserver remains development-only on port 17550 with scratch
storage, not a production authority. Native messaging uses the production pipe;
test its wire handler through the harness until VM installation.
