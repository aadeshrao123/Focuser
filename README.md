# Focuser

**Stop doomscrolling. Start doing.**

Focuser is a free, open-source website and application blocker built in Rust. Think Cold Turkey Blocker, but without the price tag and with the source code right here for you to judge.

It sits quietly in your system tray, blocks the sites you told it to block, and kills the apps you told it to kill. No cloud. No accounts. No telemetry. Just you vs. your distractions — and for once, you win.

## What it does

- **Block websites** — Add domains, keywords, wildcards, or URL paths. Or just block the entire internet and whitelist only what you need. Your call.
- **Block applications** — Steam launching itself at 2pm on a Tuesday? Not anymore. Block by executable name, path, or window title.
- **Pre-made block lists** — 1,089 domains across 13 categories (social media, games, gambling, news, porn, etc.) ready to import with one click. We did the research so you don't have to.
- **Bulk import** — Drop a text file with 500 domains and they're all blocked in under a second. Also supports JSON.
- **Exceptions (whitelist)** — Block all of reddit.com but keep r/programming? Add exceptions for specific domains that bypass your block rules.
- **Keyword blocking** — Block any URL containing "game" or "shorts" or whatever your specific weakness is. We don't judge.
- **Browser extension** — Works on Chrome, Firefox, and Edge. Shows a clean "Site Blocked" page instead of a connection error. Also lets you block the current site with one click from the toolbar.
- **Instant enforcement** — Block a site in the app, it's blocked in your browser within 2 seconds. Unblock it, same deal. No restart required.
- **Auto-elevates on Windows** — Requests admin rights on launch so it can actually modify your hosts file. No manual "Run as Administrator" needed.
- **Schedule grid** — Set blocking times per day of the week. Block social media during work hours, allow it evenings and weekends.
- **Statistics** — See what you tried to access and how many times it was blocked. The numbers are sometimes humbling.

## Screenshots

*Coming soon — the app is functional but we're still polishing the UI.*

## Tech stack

- **Rust** — Core engine, database, blocking logic, process management
- **Tauri v2** — Desktop app framework (tiny bundle, native performance)
- **SQLite** — Local database via rusqlite (your data stays on your machine)
- **Vanilla HTML/CSS/JS** — Frontend with zero framework dependencies
- **WebExtensions API (Manifest V3)** — Browser extension for Chrome, Firefox, Edge

## Platform support

| Platform | Status |
|----------|--------|
| Windows 10/11 | Tested and working |
| macOS | Builds, needs testing |
| Linux | Builds, needs testing |

The core architecture is cross-platform. Windows is the primary development target right now. macOS and Linux support is structurally there (hosts file blocking, process management via /proc and ps) but hasn't been battle-tested yet. If you're on macOS or Linux, we'd love your help testing.

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (1.80+)
- Node.js 18+ (only needed for the browser extension dev, not the app itself)

### Build and run

```bash
# Clone the repo
git clone https://github.com/aadeshrao123/Focuser.git
cd Focuser

# Build everything
cargo build --workspace

# Run the desktop app (will request admin rights on Windows)
cargo run -p focuser-ui

# Run tests
cargo test --workspace
```

### Install the browser extension

1. Open your browser's extension page:
   - **Chrome/Edge**: `chrome://extensions` or `edge://extensions`
   - **Firefox**: `about:debugging#/runtime/this-firefox`
2. Enable "Developer mode" (Chrome/Edge) or click "Load Temporary Add-on" (Firefox)
3. Load the `extension/` folder as an unpacked extension
4. Make sure the Focuser desktop app is running — the extension communicates with it on `localhost:17549`

## Project structure

```
Focuser/
├── crates/
│   ├── focuser-common/    # Shared types, errors, platform traits
│   ├── focuser-core/      # Database, rules engine, blocking logic
│   ├── focuser-service/   # Standalone service daemon (alternative to embedded)
│   ├── focuser-cli/       # Command-line interface
│   └── focuser-ui/        # Tauri desktop app (embeds the engine)
│       ├── src/           # Rust backend (commands, blocker, API server)
│       └── ui/            # Frontend (HTML/CSS/JS)
├── extension/             # Browser extension (Manifest V3)
└── CLAUDE.md              # Project conventions and coding rules
```

## How blocking works

1. **Hosts file** — Blocked domains get redirected to `127.0.0.1` in your system hosts file. This works at the OS level, before any browser even sees the request.
2. **Process monitoring** — A background thread scans running processes every 3 seconds and terminates any that match your app blocking rules.
3. **Browser extension** — Catches navigation to blocked URLs and redirects to a custom block page. Handles keyword/wildcard/URL-path rules that the hosts file can't.
4. **Local API** — The app runs an HTTP API on `127.0.0.1:17549` that the browser extension polls for rule updates.

## License

MIT License. See [LICENSE](LICENSE) for details.

Do whatever you want with this code. Fork it, modify it, sell it, use it to block your ex's social media during weak moments at 2am. We don't care. Just don't blame us if it works too well and you become unreasonably productive.

## Contributing

We need your help. Seriously.

This project was built by a small team and there's a mountain of features we want to add. Whether you're a Rust wizard, a CSS artist, or someone who just found a bug while trying to block YouTube — your contributions matter.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide, but the short version:

1. Fork the repo
2. Create a branch (`git checkout -b feature/my-cool-thing`)
3. Make your changes
4. Run `cargo test --workspace` and `cargo clippy --workspace`
5. Open a PR with a clear description

### Areas where we especially need help

- **macOS/Linux testing** — We develop on Windows. If things are broken on your OS, tell us.
- **Browser extension improvements** — Better block page, usage tracking, Firefox quirks
- **UI polish** — If you have design skills and opinions, we want both
- **Scheduling engine** — The schedule grid is visual-only right now, it needs backend wiring
- **Anti-circumvention** — Making it harder to bypass blocks (for people who want that)
- **Packaging** — Installers for Windows (MSI), macOS (DMG), Linux (AppImage, deb)
- **Translations** — The UI is English-only right now

### Found a bug?

[Open an issue](https://github.com/aadeshrao123/Focuser/issues) with:
- What you expected to happen
- What actually happened
- Your OS and browser version
- Steps to reproduce

We'll get to it. Probably faster than you expect.

## Acknowledgments

- Inspired by [Cold Turkey Blocker](https://getcoldturkey.com/) — the gold standard that we're chasing
- Built with [Tauri](https://tauri.app/), [rusqlite](https://github.com/rusqlite/rusqlite), and too much caffeine
- Pre-made block lists curated from various open-source sources

---

*If Focuser helped you get something done instead of scrolling Twitter for the 47th time today, consider starring the repo. It's free and it makes us mass unreasonably happy.*
