# Contributing to Focuser

First off — thank you. The fact that you're reading this means you're considering helping out, and that's already more than most people do. We appreciate it.

## Ground rules

- Be respectful. We're all here to build something useful.
- Keep PRs focused. One feature or fix per PR. If you're fixing a typo and also rewriting the scheduler, that's two PRs.
- Test your changes. Run `cargo test --workspace` before submitting. If you broke something, we'll find out, and it'll be awkward for everyone.

## Getting set up

```bash
# Fork and clone
git clone https://github.com/YOUR_USERNAME/Focuser.git
cd Focuser

# Build
cargo build --workspace

# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace

# Run the app
cargo run -p focuser-ui
```

### Requirements

- Rust 1.80+ (edition 2024)
- Windows 10/11 for full testing (hosts file, process killing)
- A browser for extension testing

## Code conventions

Read [CLAUDE.md](CLAUDE.md) — it has the full coding style guide. The highlights:

- **Error handling**: `thiserror` in libraries, `anyhow` in binaries. No `.unwrap()` in library code.
- **Logging**: Use `tracing` crate. Not `println!`.
- **Platform code**: Goes in `src/platform/` behind `#[cfg(target_os)]` gates.
- **Naming**: snake_case for functions, PascalCase for types, SCREAMING_SNAKE_CASE for constants.
- **Tests**: In the same file under `#[cfg(test)] mod tests`. Use `rusqlite::Connection::open_in_memory()` for DB tests.

## What to work on

### Good first issues

Look for issues tagged [`good first issue`](https://github.com/aadeshrao123/Focuser/labels/good%20first%20issue). These are intentionally scoped to be approachable.

### Feature ideas we'd love help with

- **Schedule backend** — Wire the visual schedule grid to actually enable/disable blocks on a timer
- **Pomodoro timer** — Work/break cycles with configurable durations
- **Lock types** — Timer locks, random-text-to-unlock, password protection
- **Usage statistics** — Track time spent on blocked sites (via extension), display charts
- **System tray** — Minimize to tray, quick toggle from tray icon
- **Custom block page** — Make the "Site Blocked" page configurable/themeable
- **Import/export** — Export block lists as shareable JSON files
- **macOS support** — Test and fix any platform-specific issues
- **Linux support** — Same as above. systemd integration would be great.
- **Uninstall protection** — Prevent the app from being easily killed or uninstalled during a block session
- **Installer** — Windows MSI, macOS DMG, Linux AppImage/deb

### Bug fixes

Check [open issues](https://github.com/aadeshrao123/Focuser/issues). If you find a bug that isn't listed, open an issue first so we can discuss the fix before you spend time on it.

## Pull request process

1. **Fork** the repo and create a branch from `master`
2. **Make your changes** — keep them focused
3. **Test** — `cargo test --workspace` must pass
4. **Lint** — `cargo clippy --workspace` should be clean
5. **Commit** — Write a clear commit message (see below)
6. **Push** and open a PR against `master`
7. **Describe** what you changed and why in the PR description

### Commit messages

```
feat: add pomodoro timer to scheduling engine

- Added PomodoroState struct to focuser-common types
- Timer tick logic in focuser-core engine
- UI controls for start/pause/reset
- Break notifications via system tray
```

Use prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`

Keep the subject line under 72 characters. Add a body explaining what and why.

## Reporting bugs

[Open an issue](https://github.com/aadeshrao123/Focuser/issues/new) with:

1. **What happened** — "Reddit loads even though it's in my block list"
2. **What you expected** — "Reddit should show the block page"
3. **Steps to reproduce** — Be specific. "Add reddit.com to a list and visit it" is helpful. "It doesn't work" is not.
4. **Environment** — OS, browser, Focuser version
5. **Logs** — Run with `RUST_LOG=debug cargo run -p focuser-ui` and include relevant output

## Requesting features

Open an issue with the `enhancement` label. Describe:
- What you want
- Why you want it
- How Cold Turkey or similar tools handle it (if applicable)

We're more likely to build something if you can explain the use case clearly.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

*Still reading? You're clearly the kind of person who reads documentation. We like you already. Go ahead and pick something to work on.*
