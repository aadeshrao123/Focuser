# Focuser — Project Rules

## What is this?
Focuser is an open-source, cross-platform website and application blocker (similar to Cold Turkey Blocker).
Built in Rust for maximum performance and safety. Targets Windows, macOS, and Linux.

## Architecture
- **Workspace layout**: `crates/` contains all Rust crates
  - `focuser-common` — Shared types, error types, process control, icon extraction
  - `focuser-core` — Rules engine, database, block evaluation, scheduling logic
  - `focuser-app` — Command core: every action the app can perform, in one enum
  - `focuser-cli` — Command-line interface
  - `focuser-native` — Native messaging host (browser extension bridge)
  - `focuser-devserver` — Dev-only HTTP bridge, for running the UI in a browser
  - `focuser-ui` — Tauri desktop app; owns the blocking loop and extension API
- **Docs**: `internal-docs/` holds working notes and is **gitignored — never commit or push it**.
  - `internal-docs/reference/` — long-lived project docs (FEATURES, ARCHITECTURE, ROADMAP)
  - `internal-docs/tasks/<YYYY-MM-DD-slug>/` — one folder per piece of work, each with a
    `README.md` index, numbered design docs, and a progress log recording what was done,
    what broke, and what to watch out for. Start a session by reading a task's `README.md`.
  - See `internal-docs/README.md` for conventions.

## Code Conventions
- **Edition**: Rust 2024
- **Error handling**: Use `thiserror` for library errors in common/core, `anyhow` in binaries (service/cli)
- **Async runtime**: Tokio (multi-threaded)
- **Logging**: `tracing` crate with structured logging. Use `tracing::instrument` on public functions.
- **Database**: SQLite via `rusqlite`. All migrations in `focuser-core/src/db/migrations/`.
- **Platform code**: Gate with `#[cfg(target_os = "...")]` and keep it in a
  per-OS `mod imp` inside the module that needs it — see
  `focuser-common/src/process.rs`. Callers stay platform-free.
- **Serialization**: `serde` for all data structures that cross boundaries (IPC, DB, config).
- **IDs**: UUID v4 for all entities (blocks, schedules, etc.)
- **Time**: `chrono` for all date/time. Store as UTC in DB, convert to local for display.

## Naming
- Crate names: `focuser-*` (kebab-case)
- Module names: `snake_case`
- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Error variants: `PascalCase`, descriptive (e.g., `BlockNotFound`, `DatabaseError`)

## Testing
- Unit tests in the same file (`#[cfg(test)] mod tests`)
- Integration tests in `tests/` directory per crate
- Use `tempfile` for tests that need filesystem
- Use `rusqlite::Connection::open_in_memory()` for DB tests

## Build & Run
```bash
cargo build                          # Build all crates
cargo run -p focuser-cli             # Run CLI
cargo test --workspace               # Run all tests
cargo clippy --workspace             # Lint
```

## Key Design Decisions
1. **Hosts file blocking first** — simplest, works everywhere, no driver needed
2. **SQLite for storage** — single file, no external DB, embedded with rusqlite
3. **One command core** — `focuser-app` holds every action as a `Command` variant;
   the GUI, CLI and devserver are deserialize-and-forward shims over `execute()`
4. **The GUI owns the engine** — it runs the blocking loop, hosts-file sync and the
   extension API directly. There is no daemon; a standalone service used to exist
   and was removed in 0.4.2 because nothing launched it and its IPC port collided
   with the extension API on 17549
5. **The CLI goes straight to the database** — no IPC, so it works in a shipped install
6. **TypeScript types are generated from Rust** via tauri-specta; CI fails on drift
7. **Extension-ready architecture** — browser extension support is deferred but the
   integration points are built:
   - `focuser-common/src/extension.rs` defines the full protocol (messages, rule sets, events)
   - `BlockEngine::compile_extension_rules()` compiles active rules into extension format
   - `BlockEngine::has_extension_only_rules()` detects when extension is needed
   - IPC has `GetExtensionRules`, `ExtensionEvent`, `GetCapabilities` variants
   - `BlockingCapabilities` tracks what blocking methods are available at runtime
   - When extension is added: create `focuser-native` crate (Native Messaging host binary)
     that bridges stdin/stdout JSON ↔ IPC, and the browser extension consumes `ExtensionRuleSet`

## Pre-commit Checks (MANDATORY)
Before EVERY commit and push, run ALL three checks and ensure they pass:
```bash
cargo fmt --all -- --check              # Formatting — must pass
cargo clippy --workspace -- -D warnings # Linting — zero errors
cargo test --workspace                  # Tests — all must pass
```
If any check fails, fix the issue BEFORE committing. Never push code that fails these checks.
This prevents CI failures on GitHub Actions.

## Don'ts
- Don't use `unwrap()` or `expect()` in library code — propagate errors
- Don't use `unsafe` unless absolutely necessary and document why
- Don't add dependencies without justification
- Don't put platform-specific code outside of `platform/` modules
- Don't store passwords in plaintext — use argon2 hashing

# context-mode — MANDATORY routing rules

You have context-mode MCP tools available. These rules are NOT optional — they protect your context window from flooding. A single unrouted command can dump 56 KB into context and waste the entire session.

## BLOCKED commands — do NOT attempt these

### curl / wget — BLOCKED
Any Bash command containing `curl` or `wget` is intercepted and replaced with an error message. Do NOT retry.
Instead use:
- `ctx_fetch_and_index(url, source)` to fetch and index web pages
- `ctx_execute(language: "javascript", code: "const r = await fetch(...)")` to run HTTP calls in sandbox

### Inline HTTP — BLOCKED
Any Bash command containing `fetch('http`, `requests.get(`, `requests.post(`, `http.get(`, or `http.request(` is intercepted and replaced with an error message. Do NOT retry with Bash.
Instead use:
- `ctx_execute(language, code)` to run HTTP calls in sandbox — only stdout enters context

### WebFetch — BLOCKED
WebFetch calls are denied entirely. The URL is extracted and you are told to use `ctx_fetch_and_index` instead.
Instead use:
- `ctx_fetch_and_index(url, source)` then `ctx_search(queries)` to query the indexed content

## REDIRECTED tools — use sandbox equivalents

### Bash (>20 lines output)
Bash is ONLY for: `git`, `mkdir`, `rm`, `mv`, `cd`, `ls`, `npm install`, `pip install`, and other short-output commands.
For everything else, use:
- `ctx_batch_execute(commands, queries)` — run multiple commands + search in ONE call
- `ctx_execute(language: "shell", code: "...")` — run in sandbox, only stdout enters context

### Read (for analysis)
If you are reading a file to **Edit** it → Read is correct (Edit needs content in context).
If you are reading to **analyze, explore, or summarize** → use `ctx_execute_file(path, language, code)` instead. Only your printed summary enters context. The raw file content stays in the sandbox.

### Grep (large results)
Grep results can flood context. Use `ctx_execute(language: "shell", code: "grep ...")` to run searches in sandbox. Only your printed summary enters context.

## Tool selection hierarchy

1. **GATHER**: `ctx_batch_execute(commands, queries)` — Primary tool. Runs all commands, auto-indexes output, returns search results. ONE call replaces 30+ individual calls.
2. **FOLLOW-UP**: `ctx_search(queries: ["q1", "q2", ...])` — Query indexed content. Pass ALL questions as array in ONE call.
3. **PROCESSING**: `ctx_execute(language, code)` | `ctx_execute_file(path, language, code)` — Sandbox execution. Only stdout enters context.
4. **WEB**: `ctx_fetch_and_index(url, source)` then `ctx_search(queries)` — Fetch, chunk, index, query. Raw HTML never enters context.
5. **INDEX**: `ctx_index(content, source)` — Store content in FTS5 knowledge base for later search.

## Subagent routing

When spawning subagents (Agent/Task tool), the routing block is automatically injected into their prompt. Bash-type subagents are upgraded to general-purpose so they have access to MCP tools. You do NOT need to manually instruct subagents about context-mode.

## Output constraints

- Keep responses under 500 words.
- Write artifacts (code, configs, PRDs) to FILES — never return them as inline text. Return only: file path + 1-line description.
- When indexing content, use descriptive source labels so others can `ctx_search(source: "label")` later.

## ctx commands

| Command | Action |
|---------|--------|
| `ctx stats` | Call the `ctx_stats` MCP tool and display the full output verbatim |
| `ctx doctor` | Call the `ctx_doctor` MCP tool, run the returned shell command, display as checklist |
| `ctx upgrade` | Call the `ctx_upgrade` MCP tool, run the returned shell command, display as checklist |
