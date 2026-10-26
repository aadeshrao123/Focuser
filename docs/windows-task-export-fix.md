# Task XML repair follow-up

The failed installation was inspected again after cleanup. Its service executable
had SHA-256 `0D9E9EFD71A818A845B6EE1D39031B5520B255494BBBC2318190CBE549ACC21B`,
the older build containing the invalid `LogonType:ServiceAccount` XML. Rerunning
that package recreated the failure; deleting old files alone could not correct it.

The replacement package includes the earlier SYSTEM XML correction and now also
accepts Task Scheduler exports that omit the agent's default `LeastPrivilege`
element. Explicit elevated/empty privilege values, changed owner, logon type,
installation marker, executable and arguments remain rejected. This changes task
ownership recognition, not removal authorization or commitment expiry.

`scripts/test-task-schema.ps1` validates both generated definitions using Windows
`TASK_VALIDATE_ONLY`, checks malformed definitions are rejected, and creates XML
exports using in-memory COM task definitions. It never registers/runs a task.
The Rust task-schema integration test feeds those Windows exports through the
production ownership matchers and separately covers an omitted default level.

The local recovery artifact `target/windows-task-export-fix/Patch_failed_service.ps1`
is restricted to this exact enrolled installation and original/fixed binary hashes.
It checks the live authoritative removal decision, requests a normal SCM stop,
locks storage, preserves a private complete backup, and replaces only the known
defective service executable. It leaves policy, original commitment expiry and the
pinned broker untouched. It does not launch setup or start enforcement. It is an
explicit local administrator recovery artifact, not a shipped update exception,
installer flag or general bypass. Ordinary exact-code Repair must still complete
provisioning after the correction.

The corrected service hash is
`5C4ACF2A269156AB369A64D31D3EADDBF25B7BF16C7929EB47502B4C0454DC22`.
The final installer is given a distinct filename to avoid another retry of an old
package. No extension files or unrelated features are changed.

Microsoft documents the optional privilege element and valid XML types in the
[Task Scheduler schema](https://learn.microsoft.com/en-us/windows/desktop/taskschd/task-scheduler-schema).

Validation and final package results are recorded alongside the artifacts in
`target/windows-task-export-fix`. Full installer execution, logon/reboot and
installed uninstall acceptance remain separate from nonpersistent XML checks.
