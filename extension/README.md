# Focuser browser extension

Enforces the desktop app's block lists inside the browser. Built with
[WXT](https://wxt.dev) — TypeScript, React, Vite and Tailwind, one codebase for
Chrome, Firefox, Edge, Brave and any Chromium browser.

## Why the extension exists

The desktop app can block domains through the hosts file, but a hosts file
cannot express a **keyword**, a **wildcard**, or a **URL path** rule — and it
needs administrator rights to write. The extension handles all of those and
needs no privileges, which makes it the preferred blocking path whenever it is
connected.

It also shows a real block page instead of a browser connection error.

## Commands

```bash
npm install
npm run dev              # live-reloading browser with the extension loaded
npm run dev:firefox
npm run build            # → .output/chrome-mv3/
npm run build:firefox    # → .output/firefox-mv3/
npm run zip              # store-ready archive
npm run compile          # tsc --noEmit
npm test
npm run preview          # block page in an ordinary tab, for UI work only
```

`npm run preview` renders the block page's states side by side at
<http://localhost:5199>. It has no `browser.*` access — it is for looks, not
behaviour.

## Layout

```
entrypoints/
  background.ts       matches navigations, injects the block page, ticks allowances
  early.content.ts    document_start: hides the page before it paints
  block-page.ts       unlisted script — replaces a blocked page in place
  popup/              React popup
lib/
  rules.ts            pure matching. No browser APIs, fully tested
  categories.ts       host → category → escalating message
  api.ts              the desktop app's local HTTP API
  messages.ts         typed request/reply contract
components/
  BlockPage.tsx       the page a blocked user actually reads
```

## Decisions worth knowing

**The block page is injected, not navigated to.** A redirect to an extension
page loses the URL the user typed, pollutes history, and makes "back" return to
the blocked site. Injecting keeps the address bar honest.

**The page is hidden at `document_start` and revealed if allowed** — otherwise
there is a flash of the blocked site, which is exactly the moment of temptation
the product exists to remove. It **fails open**: if the background never
answers within two seconds, the page is shown. A blocker that blanks every tab
when it breaks is worse than one that occasionally misses.

**The block UI lives in a shadow root.** The "page" being styled is an
arbitrary site we just took over, so isolation goes both ways.

**Messages escalate with the visit count.** A single fixed string stops being
read after the second visit. Roughly 40% of them carry `{count}` / `{domain}`
placeholders — `messageFor` substitutes them, and a test fails if any rendered
message still contains braces.

**No Native Messaging.** The old extension tried a native host first and fell
back to HTTP after three failures. That host has had nothing to connect to
since the standalone service was removed, so the attempt only cost startup
latency. HTTP on `127.0.0.1:17549` is the only transport.

**Permissions were trimmed.** `webRequest` and `nativeMessaging` were declared
and never used; both cost store-review scrutiny for nothing.

## Privacy

Everything stays on the machine. The extension talks to `127.0.0.1:17549` and
nothing else — there is no remote endpoint anywhere in this codebase. Category
labels come from a file bundled with the extension, not from a lookup service.

## Publishing

WXT can submit to the stores directly:

```bash
npm run zip && npm run zip:firefox
npx wxt submit init      # once, to store credentials in .env.submit
npx wxt submit --dry-run --chrome-zip .output/*-chrome.zip --firefox-zip .output/*-firefox.zip
```

The Firefox build is **MV3**, matching the published AMO listing. WXT defaults
Firefox to MV2; `manifestVersion: 3` in `wxt.config.ts` overrides that, and
dropping it would be a regression for existing users.
