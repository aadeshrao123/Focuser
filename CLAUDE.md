# Focuser — Project Rules

## What is this?
Focuser is an open-source, cross-platform website and application blocker (similar to Cold Turkey Blocker).
Built in Rust for maximum performance and safety. Targets Windows, macOS, and Linux.

## Architecture
- **Workspace layout**: `crates/` contains all Rust crates
  - `focuser-common` — Shared types, error types, constants, platform abstractions
  - `focuser-core` — Rules engine, database, block evaluation, scheduling logic
  - `focuser-service` — System daemon/service with platform-specific blocking
  - `focuser-cli` — Command-line interface
  - `focuser-ui` — Tauri GUI (future)
- **Docs**: `docs/` contains FEATURES.md, ARCHITECTURE.md, ROADMAP.md

## Code Conventions
- **Edition**: Rust 2024
- **Error handling**: Use `thiserror` for library errors in common/core, `anyhow` in binaries (service/cli)
- **Async runtime**: Tokio (multi-threaded)
- **Logging**: `tracing` crate with structured logging. Use `tracing::instrument` on public functions.
- **Database**: SQLite via `rusqlite`. All migrations in `focuser-core/src/db/migrations/`.
- **Platform code**: Gate with `#[cfg(target_os = "...")]` in `focuser-service/src/platform/`.
  Common trait in `focuser-common/src/platform.rs`, implemented per OS.
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
cargo run -p focuser-service         # Run service (needs admin/root)
cargo test --workspace               # Run all tests
cargo clippy --workspace             # Lint
```

## Key Design Decisions
1. **Hosts file blocking first** — simplest, works everywhere, no driver needed
2. **SQLite for storage** — single file, no external DB, embedded with rusqlite
3. **IPC via named pipes (Windows) / Unix sockets (Linux/macOS)** — fast, local-only
4. **Service runs as elevated/root** — required for hosts file and process control
5. **CLI communicates with service over IPC** — CLI never modifies system directly
6. **Modular platform traits** — each OS implements `PlatformBlocker` trait
7. **Extension-ready architecture** — browser extension support is deferred but the
   integration points are built:
   - `focuser-common/src/extension.rs` defines the full protocol (messages, rule sets, events)
   - `BlockEngine::compile_extension_rules()` compiles active rules into extension format
   - `BlockEngine::has_extension_only_rules()` detects when extension is needed
   - IPC has `GetExtensionRules`, `ExtensionEvent`, `GetCapabilities` variants
   - `BlockingCapabilities` tracks what blocking methods are available at runtime
   - When extension is added: create `focuser-native` crate (Native Messaging host binary)
     that bridges stdin/stdout JSON ↔ IPC, and the browser extension consumes `ExtensionRuleSet`

## Don'ts
- Don't use `unwrap()` or `expect()` in library code — propagate errors
- Don't use `unsafe` unless absolutely necessary and document why
- Don't add dependencies without justification
- Don't put platform-specific code outside of `platform/` modules
- Don't store passwords in plaintext — use argon2 hashing
