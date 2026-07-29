<p align="center">
  <img src="assets/branding/focuser-banner.png" alt="Focuser — stop doomscrolling, start doing. Free and open source. Blocks websites and applications. Never leaves your computer." width="100%">
</p>

<p align="center">
  <a href="https://github.com/aadeshrao123/Focuser/releases">Download</a> &middot;
  <a href="https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj">Chrome Extension</a> &middot;
  <a href="https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/">Firefox Extension</a> &middot;
  <a href="TRANSLATING.md">Translate it</a>
</p>

---

Focuser is a free, open-source website and application blocker built in Rust. Think Cold Turkey Blocker, but without the price tag and with the source code right here for you to judge.

It sits quietly in your system tray, blocks the sites you told it to block, and kills the apps you told it to kill. No cloud. No accounts. No telemetry. Just you vs. your distractions, and for once, you win.

It speaks ten languages: English, Español, Français, Deutsch, Português, Italiano, Русский, 中文, 日本語 and 한국어.

## Screenshots

### The desktop app

<details>
<summary><strong>Dashboard</strong> - What is blocked right now, and a Pomodoro timer to start a session</summary>
<br>
<p>Four counters across the top, then the focus session: three dials for focus, short break and long break, how many cycles before the long one, and a bar showing what the whole session adds up to. Block lists can be flipped on and off without leaving the page.</p>
<img src="assets/screenshots/dashboard.png" alt="Dashboard showing blocked attempts, a Pomodoro focus session and block list toggles" width="100%">
</details>

<details>
<summary><strong>Block Lists</strong> - Group sites and apps, then schedule and lock each group on its own</summary>
<br>
<p>A list is the unit everything else hangs off. Each one has its own websites, applications, schedule and exceptions, and can be locked so you cannot casually undo it.</p>
<img src="assets/screenshots/block-lists.png" alt="Block Lists page with two lists, each showing site and app counts" width="100%">
</details>

<details>
<summary><strong>Websites</strong> - Domains, keywords, wildcards, URL paths, or the whole internet</summary>
<br>
<p>Pick the kind of rule and the hint underneath tells you what it will actually match. Starter lists sit right next to the add box: choose a category, hit Import, and the sites that are already there are skipped rather than duplicated.</p>
<img src="assets/screenshots/websites.png" alt="Websites page with the rule-kind picker, the starter list importer and a list of blocked domains" width="100%">
</details>

<details>
<summary><strong>Applications</strong> - Block programs by executable, full path, or window title</summary>
<br>
<p>Blocked programs show their real icon, read out of the executable on your own machine. Nothing is fetched from the internet to do it.</p>
<img src="assets/screenshots/applications.png" alt="Applications page showing a blocked executable with its real icon" width="100%">
</details>

<details>
<summary><strong>Schedule</strong> - Always on, or painted onto a weekly grid</summary>
<br>
<p>Leave a list always active, or switch to the grid and drag to paint the hours you want it blocking. Presets cover work hours, evenings and weekends. The summary underneath adds up your week and warns you when there are unsaved changes.</p>
<img src="assets/screenshots/schedule-always-active.png" alt="Schedule set to always active, showing 168 of 168 hours covered" width="100%">
<br><br>
<img src="assets/screenshots/schedule-weekly.png" alt="Weekly schedule grid with work hours painted in, showing 76 of 168 hours covered" width="100%">
</details>

<details>
<summary><strong>Allowances</strong> - A daily budget instead of an outright block</summary>
<br>
<p>Give a site or app a number of minutes per day. When the budget runs out it is blocked for the rest of the day. Time can be counted only while the tab or window is actually focused, so a forgotten background tab does not drain it.</p>
<img src="assets/screenshots/allowances.png" alt="Allowances page showing a 30 minute daily budget with 15 minutes used" width="100%">
</details>

<details>
<summary><strong>Statistics</strong> - What you tried to open, and how often you got stopped</summary>
<br>
<p>Three views of the same data. <strong>Bars</strong> is attempts per day. <strong>Individual</strong> follows one site over time with its own share of the total. <strong>Detailed</strong> puts every site in one chart. Each site keeps the same colour every time, derived from a hash of its name.</p>
<img src="assets/screenshots/statistics-bars.png" alt="Statistics in Bars view, with blocked attempts per day and a most-blocked table" width="100%">
<br><br>
<img src="assets/screenshots/statistics-individual.png" alt="Statistics in Individual view, tracking one site over the last seven days" width="100%">
</details>

<details>
<summary><strong>Settings</strong> - Startup, browsers, extension status, and your data</summary>
<br>
<p>Shows which browsers have the extension connected and offers to install it where they do not. Export every block list to a file or replace them from one, choose how long statistics are kept, and delete everything if you want to start over.</p>
<img src="assets/screenshots/settings.png" alt="Settings page showing startup, browser, extension and data options" width="100%">
</details>

### The browser extension

<details>
<summary><strong>Block page</strong> - Names the site, says which rule caught it, and counts how many times you have tried today</summary>
<br>
<p>The block happens before the page is drawn, so you never catch a glimpse of what you were avoiding. The message changes as the count climbs — the first visit of the day reads differently from the fourteenth.</p>
<img src="assets/screenshots/extension/block-page.png" alt="Block page for reddit.com, showing the category, the matching rule and a first-visit-today badge" width="100%">
<br><br>
<img src="assets/screenshots/extension/block-page-repeat.png" alt="Block page for youtube.com on the fourteenth attempt of the day, with a different message" width="100%">
<br><br>
<p>Keyword rules say which word matched, not just that something did.</p>
<img src="assets/screenshots/extension/block-page-keyword.png" alt="Block page for a gambling site, showing that the address contained a blocked word" width="100%">
</details>

<details>
<summary><strong>Toolbar popup</strong> - Block the site you are on, or take it out of every list at once</summary>
<br>
<p>Shows which of your lists a site is actually in, rather than guessing one. Unblocking removes it from all of them. If the desktop app is closed the icon carries a red mark and the popup says so plainly, instead of pretending to work.</p>
<img src="assets/screenshots/extension/popup.png" alt="Extension popup showing rules active, today's counts, the current site and a block button" width="100%">
</details>

<details>
<summary><strong>Welcome page</strong> - What to set up, and what changed in this version</summary>
<br>
<p>Shown once on install and again after an update. Explains the one thing that needs setting up, which is keeping the desktop app running, and lays out the four kinds of rule side by side.</p>
<img src="assets/screenshots/extension/welcome.png" alt="Extension welcome page explaining that the extension asks the desktop app for block lists" width="100%">
</details>

## What it does

- **Block websites** - Add domains, keywords, wildcards, or URL paths. Or just block the entire internet and whitelist only what you need. Your call. Keywords, wildcards and URL paths are enforced by the browser extension, since a hosts file has no way to express a pattern. Focuser tells you on the Websites page when you have one of those rules and no extension installed.
- **Block applications** - Steam launching itself at 2pm on a Tuesday? Not anymore. Block by executable name, path, or window title.
- **Real application icons** - Blocked programs show their actual icon, read out of the executable on your own machine. Nothing is fetched from the internet to do it, because asking an icon service once per entry would hand over your entire block list.
- **Pre-made block lists** - 1,207 domains across 13 categories (social media, games, gambling, news, adult, etc.) ready to import with one click. We did the research so you don't have to.
- **Bulk import** - Drop a text file with 500 domains and they're all blocked in under a second. Also supports JSON.
- **Exceptions (whitelist)** - Block all of reddit.com but keep r/programming? Add exceptions for specific domains that bypass your block rules.
- **Keyword blocking** - Block any URL containing "game" or "shorts" or whatever your specific weakness is. We don't judge.
- **Focus Lock** - Lock a block list for a set duration. Once locked, you can't disable it, delete it, or edit it until the timer runs out. For when you genuinely don't trust yourself.
- **Pomodoro focus sessions** - Work for 25 minutes, break for 5, repeat — blocks toggle on and off automatically with each phase. After 4 work cycles you earn a longer break. Pick a preset (Classic, Long, Sprint) or set your own rhythm. The dashboard shows a live ring counting down with pause / skip / stop controls.
- **Daily allowance quotas** - Cap a site at N minutes per day. YouTube = 30 min/day. Reddit = 15. Whatever. The site stays accessible until you've burned through your quota, then it's blocked until midnight. Strict mode counts only the focused tab; loose mode counts any open tab.
- **Smart focus interaction** - Pomodoro and allowances are aware of each other. During a work phase, allowance back doors close — no sneaking off to YouTube even if you have minutes left. Hit pause and your allowances kick back in. Resume and they suspend again.
- **Weekly schedule** - Paint the hours a list is active on a 7×24 grid, or click a day or hour heading to fill a whole row or column. Block social media during work hours, allow it evenings and weekends. Or go 24/7 and be done with it.
- **Browser extension** - Available on [Chrome Web Store](https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj) and [Firefox Add-ons](https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/). Also works on Edge, Brave, and Opera. Shows a clean "Site Blocked" page instead of a connection error.
- **Browser enforcement** - If a browser is running without the extension installed, Focuser will close it and show you an install prompt with a direct link to the store page. No more loopholes.
- **Instant enforcement** - Block a site in the app, it's blocked in your browser within 2 seconds. Unblock it, same deal. No restart required.
- **Statistics** - See what you tried to access, how many times it was blocked, and track it across days. The numbers are sometimes humbling.
- **Auto-elevates on Windows** - Requests admin rights on launch so it can actually modify your hosts file. No manual "Run as Administrator" needed.
- **System tray** - Runs in the background after you close the window. Double-click the tray icon to bring it back. Closing the app doesn't stop the blocking.
- **Import/Export** - Export your entire config (block lists, rules, schedules, exceptions) to a file. Import it on another machine. Move between computers without starting over.
- **Ten languages** - English, Spanish, French, German, Portuguese, Italian, Russian, Chinese, Japanese and Korean, across the whole app and the whole extension — window, tray menu, block page, popup and welcome page. The app follows a setting; the extension follows your browser, because `browser.i18n` has no way to change locale while running. Plurals go through `Intl.PluralRules`, so Russian gets one/few/many and Japanese gets the single form it actually uses.

## Privacy

Focuser doesn't phone home. No analytics, no crash reporting, no usage tracking, no accounts, no cloud sync. Your block lists and stats live in a SQLite file on your machine and nowhere else.

That principle has teeth in places you might not expect. Application icons are read from the executables already on your disk rather than pulled from an icon service, because a request per entry would tell that service exactly what you are trying to avoid. See [PRIVACY.md](PRIVACY.md) for the full policy.

## Tech stack

- **Rust** (edition 2024) - Core engine, database, blocking logic, process management
- **Tauri v2** - Desktop app framework (tiny bundle, native performance)
- **SQLite** - Local database via rusqlite
- **React 19 + TypeScript + Tailwind CSS 4** - Frontend, built with Vite
- **WXT + React + TypeScript** - Browser extension (Manifest V3) for Chrome, Firefox, Edge, Brave, Opera

## Platform support

| Platform | Status |
|----------|--------|
| Windows 10/11 | Tested and working |
| macOS | Builds and passes CI, needs real-world testing |
| Linux | Builds and passes CI, needs real-world testing |

The core architecture is cross-platform and the test suite runs on all three in CI. Windows is the primary development target, so macOS and Linux are structurally there (hosts file blocking, process management, icon extraction) but haven't been put through their paces by an actual human. If you're on either, we'd love your help.

## Architecture

Every action the app can perform is one variant of a single `Command` enum, run by a single `execute()` in `focuser-app`. The desktop app, the CLI, the background service and the dev server are all thin shims over it — none of them holds logic of its own.

This isn't architecture for its own sake. Before it existed, the GUI and the service had quietly drifted into disagreeing about what "stop a block list" meant, and each grew its own bugs. Now there is one answer and four ways to ask for it.

The TypeScript types the frontend uses are **generated** from the Rust types by tauri-specta. Change a Rust struct and the frontend stops compiling until it's updated; CI fails if the checked-in bindings drift.

```
Focuser/
├── crates/
│   ├── focuser-common/    # Shared types, errors, process control, icon extraction
│   ├── focuser-core/      # Database, rules engine, blocking logic
│   ├── focuser-app/       # Command core — every action the app can perform
│   ├── focuser-native/    # Native messaging host (extension bridge)
│   ├── focuser-cli/       # Command-line interface
│   ├── focuser-devserver/ # Dev-only HTTP bridge, for running the UI in a browser
│   └── focuser-ui/        # Tauri desktop app
│       ├── src/           # Rust shell (blocker, extension API, native shims)
│       └── frontend/      # React + TypeScript + Tailwind
├── extension/             # Browser extension (WXT + React + TypeScript, MV3)
└── assets/                # Icons, branding, screenshots
```

## How blocking works

1. **Hosts file** - Blocked domains get redirected to `127.0.0.1` in your system hosts file. This works at the OS level, before any browser even sees the request.
2. **Process monitoring** - A background thread scans running processes and terminates any that match your app blocking rules.
3. **Browser extension** - Catches navigation to blocked URLs and replaces the page with a block screen. Handles keyword, wildcard, and URL-path rules the hosts file can't.
4. **Browser enforcement** - Detects browsers running without the Focuser extension. After a grace period, it closes the browser and prompts you to install it.
5. **Local API** - The app runs an HTTP API on `127.0.0.1:17549` that the extension polls for rule updates. Everything stays local.
6. **Pomodoro + allowance overlay** - Pomodoro toggles a block list at each work/break boundary. Allowances track per-domain time from tab activity reported by the extension, and inject themselves as exceptions until the daily quota runs out — at which point the domain flips back into the blocked set until midnight.

One rule worth knowing: `youtube.com`, `www.youtube.com` and `m.youtube.com` are the same site everywhere in Focuser. Hostnames are canonicalised in exactly one place (`focuser_common::host`) and every rule, exception and allowance goes through it, so a block or an allowance can't apply to one form and miss another.

## Getting started

### Download

Grab the latest installer from the [Releases](https://github.com/aadeshrao123/Focuser/releases) page. Run it, install the browser extension when prompted, and you're good to go.

### Build from source

**Prerequisites:** [Rust](https://rustup.rs/) 1.85+ (edition 2024) and [Node.js](https://nodejs.org/) 20+ for the frontend.

```bash
git clone https://github.com/aadeshrao123/Focuser.git
cd Focuser

cargo build --workspace
cargo test --workspace

# Desktop app (requests admin rights on Windows)
cargo run -p focuser-ui
```

Building installers additionally needs the sidecar binaries in place:

```bash
cargo build --release -p focuser-cli -p focuser-native
# copy each into crates/focuser-ui/binaries/<name>-<target-triple>[.exe]
cd crates/focuser-ui && npx --yes @tauri-apps/cli@2 build
```

`npx` rather than `cargo install tauri-cli`: the CLI ships as a prebuilt binary on npm, so
this is a few seconds instead of the several minutes it takes to compile. `cargo tauri
build` works too if you already have it installed.

### Developing the UI without rebuilding the app

The frontend can run in an ordinary browser against the **real** Rust command core, which makes it reachable by normal web tooling instead of only by clicking around a desktop window:

```bash
cargo run -p focuser-devserver -- --memory     # HTTP bridge on :17550
cd crates/focuser-ui/frontend && npm run dev   # Vite on :1420
```

Both paths end in the same `execute()`, so what you exercise in the browser is the actual backend, not a mock that quietly drifts from it.

```bash
npm run typecheck   # tsc
npm run lint        # biome
npm test            # vitest
npm run bindings    # regenerate the TypeScript types from Rust
```

### Browser extension

Install from the store (recommended):
- **Chrome / Edge / Brave / Opera**: [Chrome Web Store](https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj)
- **Firefox**: [Firefox Add-ons](https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/)

Or build and load it yourself:

```bash
cd extension
npm install
npm run build                        # produces extension/.output/chrome-mv3/
npm run build:firefox                # produces extension/.output/firefox-mv3/
npm test
npm run dev                          # live-reloading browser with the extension loaded
npm run preview                      # block page, popup and welcome page in an ordinary
                                     # tab, with a picker for all ten languages
```

- **Chrome**: `chrome://extensions` → Developer mode → Load unpacked → `extension/.output/chrome-mv3/`
- **Firefox**: `about:debugging` → This Firefox → Load Temporary Add-on → `extension/.output/firefox-mv3/manifest.json` (session-only; permanent installation needs AMO signing)

Disable the store version first — same extension ID, and you'd be testing old code.

## Contributing

We need your help. Seriously.

This project was built by a small team and there's a mountain of features we want to add. Whether you're a Rust wizard, a CSS artist, or someone who just found a bug while trying to block YouTube, your contributions matter.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide, but the short version:

1. Fork the repo
2. Create a branch (`git checkout -b feature/my-cool-thing`)
3. Make your changes
4. Run the checks below — CI runs the same ones on Windows, macOS and Linux
5. Open a PR with a clear description

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd extension && npm test
cd crates/focuser-ui/frontend && npm run typecheck && npm run lint && npm test
```

### Areas where we especially need help

- **macOS/Linux testing** - We develop on Windows. The suite passes in CI on all three, but CI isn't a person using the thing. If it's broken on your OS, tell us.
- **Application icons on macOS and Linux** - The code is there and grounded in the published specs, but macOS apps that ship their icon only inside a compiled asset catalog aren't covered yet.
- **Browser extension improvements** - Better block page, usage tracking, Firefox quirks
- **UI polish** - If you have design skills and opinions, we want both
- **Anti-circumvention** - Making it harder to bypass blocks (for people who want that)
- **Translations** - Ten languages ship, and every one but English is a first draft written without a native speaker checking it. Fixing an awkward sentence in a language we already have is more useful than adding an eleventh. See [TRANSLATING.md](TRANSLATING.md); it is one file and no Rust.

### Found a bug?

[Open an issue](https://github.com/aadeshrao123/Focuser/issues) with:
- What you expected to happen
- What actually happened
- Your OS and browser version
- Steps to reproduce

We'll get to it. Probably faster than you expect.

## License

MIT License. See [LICENSE](LICENSE) for details.

Do whatever you want with this code. Fork it, modify it, sell it, use it to block your ex's social media during weak moments at 2am. We don't care. Just don't blame us if it works too well and you become unreasonably productive.

## Acknowledgments

- Inspired by [Cold Turkey Blocker](https://getcoldturkey.com/), the gold standard we're chasing
- Built with [Tauri](https://tauri.app/), [rusqlite](https://github.com/rusqlite/rusqlite), and too much caffeine
- Pre-made block lists curated from various open-source sources

---

*If Focuser helped you get something done instead of scrolling Twitter for the 47th time today, consider starring the repo. It's free and it makes us unreasonably happy.*
