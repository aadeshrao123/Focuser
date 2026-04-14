<p align="center">
  <img src="assets/branding/focuser-icon-256.png" alt="Focuser" width="128" height="128">
</p>

<h1 align="center">Focuser</h1>

<p align="center"><strong>Stop doomscrolling. Start doing.</strong></p>

<p align="center">
  <a href="https://github.com/aadeshrao123/Focuser/releases">Download</a> &middot;
  <a href="https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj">Chrome Extension</a> &middot;
  <a href="https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/">Firefox Extension</a>
</p>

---

Focuser is a free, open-source website and application blocker built in Rust. Think Cold Turkey Blocker, but without the price tag and with the source code right here for you to judge.

It sits quietly in your system tray, blocks the sites you told it to block, and kills the apps you told it to kill. No cloud. No accounts. No telemetry. Just you vs. your distractions, and for once, you win.

## Screenshots

<details>
<summary><strong>Dashboard</strong> - Your blocking overview at a glance</summary>
<br>
<img src="assets/screenshots/dashboard.png" alt="Dashboard" width="100%">
</details>

<details>
<summary><strong>Block Lists</strong> - Organize blocks into groups with Focus Lock protection</summary>
<br>
<img src="assets/screenshots/block-lists.png" alt="Block Lists" width="100%">
</details>

<details>
<summary><strong>Websites</strong> - Domains, keywords, wildcards, pre-made lists, bulk import</summary>
<br>
<img src="assets/screenshots/websites.png" alt="Websites" width="100%">
</details>

<details>
<summary><strong>Schedule</strong> - 24/7 blocking or a weekly time grid, your call</summary>
<br>
<img src="assets/screenshots/schedule-24-7.png" alt="Schedule - Always Active" width="100%">
<br><br>
<img src="assets/screenshots/schedule-weekly.png" alt="Schedule - Weekly Grid" width="100%">
</details>

<details>
<summary><strong>Focus Sessions</strong> - Pomodoro work/break cycles and per-site daily allowance quotas, both right on the dashboard</summary>
<br>
<em>Screenshot coming in the next release. For now: think a circular timer counting down on the left, your live allowance bars on the right.</em>
</details>

<details>
<summary><strong>Statistics</strong> - See what you tried to access and how many times you got stopped</summary>
<br>
<img src="assets/screenshots/statistics.png" alt="Statistics" width="100%">
</details>

<details>
<summary><strong>Settings</strong> - Import/export configs, data retention, the usual</summary>
<br>
<img src="assets/screenshots/settings.png" alt="Settings" width="100%">
</details>

## What it does

- **Block websites** - Add domains, keywords, wildcards, or URL paths. Or just block the entire internet and whitelist only what you need. Your call.
- **Block applications** - Steam launching itself at 2pm on a Tuesday? Not anymore. Block by executable name, path, or window title.
- **Pre-made block lists** - 1,089 domains across 13 categories (social media, games, gambling, news, porn, etc.) ready to import with one click. We did the research so you don't have to.
- **Bulk import** - Drop a text file with 500 domains and they're all blocked in under a second. Also supports JSON.
- **Exceptions (whitelist)** - Block all of reddit.com but keep r/programming? Add exceptions for specific domains that bypass your block rules.
- **Keyword blocking** - Block any URL containing "game" or "shorts" or whatever your specific weakness is. We don't judge.
- **Focus Lock** - Lock a block list for a set duration. Once locked, you can't disable it, delete it, or edit it until the timer runs out. For when you genuinely don't trust yourself.
- **Pomodoro focus sessions** - Work for 25 minutes, break for 5, repeat — blocks toggle on and off automatically with each phase. After 4 work cycles you earn a longer break. Pick a preset (Classic, Long, Sprint) or set your own rhythm. The dashboard shows a live ring counting down with pause / skip / stop controls. The original productivity technique, finally not living in a separate app.
- **Daily allowance quotas** - Cap a site at N minutes per day. YouTube = 30 min/day. Reddit = 15. Whatever. The site stays accessible until you've burned through your quota, then it gets blocked until midnight. For sites you actually use but don't want to live on. Strict mode counts only the focused tab; loose mode counts any open tab.
- **Smart focus interaction** - Pomodoro and allowances are aware of each other. During a work phase, allowance back doors close — no sneaking off to YouTube even if you have minutes left. Hit pause and your allowances kick back in. Resume and they suspend again. Outside a session, allowances behave normally.
- **Browser extension** - Available on [Chrome Web Store](https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj) and [Firefox Add-ons](https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/). Also works on Edge, Brave, and Opera. Shows a clean "Site Blocked" page instead of a connection error.
- **Browser enforcement** - If a browser is running without the extension installed, Focuser will close it and show you an install prompt with a direct link to the store page. No more loopholes.
- **Instant enforcement** - Block a site in the app, it's blocked in your browser within 2 seconds. Unblock it, same deal. No restart required.
- **Schedule grid** - Set blocking times per day of the week. Block social media during work hours, allow it evenings and weekends. Or just go 24/7 and be done with it.
- **Statistics and timeline** - See what you tried to access, how many times it was blocked, and track it across days. The numbers are sometimes humbling.
- **Auto-elevates on Windows** - Requests admin rights on launch so it can actually modify your hosts file. No manual "Run as Administrator" needed.
- **System tray** - Runs in the background after you close the window. Double-click the tray icon to bring it back. Closing the app doesn't stop the blocking.
- **Import/Export** - Export your entire config (block lists, rules, schedules, exceptions) to a file. Import it on another machine. Move between computers without starting over.

## Tech stack

- **Rust** - Core engine, database, blocking logic, process management
- **Tauri v2** - Desktop app framework (tiny bundle, native performance)
- **SQLite** - Local database via rusqlite (your data stays on your machine)
- **Vanilla HTML/CSS/JS** - Frontend with zero framework dependencies
- **WebExtensions API (Manifest V3)** - Browser extension for Chrome, Firefox, Edge, Brave, Opera

## Platform support

| Platform | Status |
|----------|--------|
| Windows 10/11 | Tested and working |
| macOS | Builds, needs testing |
| Linux | Builds, needs testing |

The core architecture is cross-platform. Windows is the primary development target right now. macOS and Linux support is structurally there (hosts file blocking, process management via /proc and ps) but hasn't been battle-tested yet. If you're on macOS or Linux, we'd love your help testing.

## Getting started

### Download

Grab the latest installer from the [Releases](https://github.com/aadeshrao123/Focuser/releases) page. Run it, install the browser extension when prompted, and you're good to go.

### Build from source

**Prerequisites:** [Rust](https://rustup.rs/) (1.80+)

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

### Browser extension

Install from the store (recommended):
- **Chrome / Edge / Brave / Opera**: [Chrome Web Store](https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj)
- **Firefox**: [Firefox Add-ons](https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/)

Or load manually for development:
1. Open your browser's extension page (`chrome://extensions`, `edge://extensions`, or `about:debugging` in Firefox)
2. Enable "Developer mode" (Chromium browsers) or click "Load Temporary Add-on" (Firefox)
3. Load the `extension/dist/chrome/` folder (Chromium) or `extension/` folder (Firefox)
4. Make sure the Focuser desktop app is running

## Project structure

```
Focuser/
├── crates/
│   ├── focuser-common/    # Shared types, errors, platform traits
│   ├── focuser-core/      # Database, rules engine, blocking logic
│   ├── focuser-service/   # Standalone service daemon
│   ├── focuser-native/    # Native messaging host (extension bridge)
│   ├── focuser-cli/       # Command-line interface
│   └── focuser-ui/        # Tauri desktop app (embeds the engine)
│       ├── src/           # Rust backend (commands, blocker, API server)
│       └── ui/            # Frontend (HTML/CSS/JS)
├── extension/             # Browser extension (Manifest V3)
│   ├── dist/chrome/       # Chrome/Edge/Brave/Opera build
│   └── dist/firefox/      # Firefox build
└── assets/                # Icons, branding, screenshots
```

## How blocking works

1. **Hosts file** - Blocked domains get redirected to `127.0.0.1` in your system hosts file. This works at the OS level, before any browser even sees the request.
2. **Process monitoring** - A background thread scans running processes every 3 seconds and terminates any that match your app blocking rules.
3. **Browser extension** - Catches navigation to blocked URLs and replaces the page with a block screen. Handles keyword, wildcard, and URL-path rules that the hosts file can't.
4. **Browser enforcement** - Detects browsers running without the Focuser extension. After a 60-second grace period, it closes the browser and prompts you to install the extension from the store.
5. **Local API** - The app runs an HTTP API on `127.0.0.1:17549` that the browser extension polls for rule updates. Everything stays local.
6. **Pomodoro + Allowance overlay** - Pomodoro sessions toggle a chosen block list's enabled flag at each work/break boundary. Allowances track per-domain time via tab activity reported by the extension (5s ticks while awake, 30s alarm-driven ticks when the service worker sleeps), and inject themselves as exceptions into the rule set until the daily quota runs out — at which point the domain flips back into the blocked set until midnight local time.

## Privacy

Focuser doesn't phone home. There's no analytics, no crash reporting, no usage tracking, no accounts, and no cloud sync. Your block lists and browsing stats live in a SQLite database on your machine and nowhere else. See [PRIVACY.md](PRIVACY.md) for the full policy.

## License

MIT License. See [LICENSE](LICENSE) for details.

Do whatever you want with this code. Fork it, modify it, sell it, use it to block your ex's social media during weak moments at 2am. We don't care. Just don't blame us if it works too well and you become unreasonably productive.

## Contributing

We need your help. Seriously.

This project was built by a small team and there's a mountain of features we want to add. Whether you're a Rust wizard, a CSS artist, or someone who just found a bug while trying to block YouTube, your contributions matter.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide, but the short version:

1. Fork the repo
2. Create a branch (`git checkout -b feature/my-cool-thing`)
3. Make your changes
4. Run `cargo test --workspace` and `cargo clippy --workspace`
5. Open a PR with a clear description

### Areas where we especially need help

- **macOS/Linux testing** - We develop on Windows. If things are broken on your OS, tell us.
- **Browser extension improvements** - Better block page, usage tracking, Firefox quirks
- **UI polish** - If you have design skills and opinions, we want both
- **Anti-circumvention** - Making it harder to bypass blocks (for people who want that)
- **Translations** - The UI is English-only right now

### Found a bug?

[Open an issue](https://github.com/aadeshrao123/Focuser/issues) with:
- What you expected to happen
- What actually happened
- Your OS and browser version
- Steps to reproduce

We'll get to it. Probably faster than you expect.

## Acknowledgments

- Inspired by [Cold Turkey Blocker](https://getcoldturkey.com/), the gold standard that we're chasing
- Built with [Tauri](https://tauri.app/), [rusqlite](https://github.com/rusqlite/rusqlite), and too much caffeine
- Pre-made block lists curated from various open-source sources

---

*If Focuser helped you get something done instead of scrolling Twitter for the 47th time today, consider starring the repo. It's free and it makes us unreasonably happy.*
