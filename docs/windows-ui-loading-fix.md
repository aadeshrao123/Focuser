# Desktop commands stuck at Loading

The installed authority and session agent were running. A normal unelevated
production CLI `--json list ls` request returned the stored lists successfully
in 835 milliseconds, so the UI failure was not an unavailable database/service.

The pinned Tauri macro implementation (`tauri-macros 2.5.5`, Tauri 2.10.3)
executes `#[tauri::command(async)]` synchronous functions inside an async task.
It does **not** move their bodies to a blocking worker. The desktop handler
called the blocking IPC client, which creates and blocks on its own Tokio
runtime. Doing that within Tauri's runtime panics. The command wrapper never
reaches its response callback, leaving React's queries pending indefinitely.
The desktop update handlers had the same runtime nesting problem.

The main command handler now explicitly awaits a blocking worker. Worker
failures become a serializable command error, so the frontend receives a reply.
Windows login-preference and update commands use the same safe worker boundary.
Policy evaluation, SQLite, commitment expiry and all authorization remain in the
running service. No service/client protocol or generated command type changes.

The frontend also explicitly schedules local queries and mutations regardless
of Windows' internet status. TanStack Query's default online-only scheduling
could otherwise leave local commands paused at Loading without even calling
IPC. An unavailable authority still returns an error; there is no writable
database or replacement-engine fallback and mutations are not automatically
retried.

Validation includes the actual UI worker source against a scratch Windows pipe
authority while already inside an async command handler, bounded unavailable
authority failure, and serialization of worker failures. Frontend tests cover
offline reads, offline mutations, and error display state without retries.
Neither harness installs or activates enforcement. Full installer build/audit
and local client repair results are recorded under
`target/windows-ui-loading-fix`.

The installed repair replaces only the desktop executable, preserving the
running service, independent session agent, policies, authoritative data and
browser extension. It is a one-off administrator file repair for the known
unsigned local build, not an updater authorization exception or a new phase.
