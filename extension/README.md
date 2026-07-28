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

`npm run preview` renders the block page and the popup at
<http://localhost:5199>, one state at a time, switched from the bar at the top.
The harness stubs `browser.*` and the desktop app's HTTP API so the popup can
be seen connected, disconnected and with no lists — production code does not
know it is being previewed.

It shows how things **look**, not whether they **work**. To check the real
built artifact — DOM replacement, shadow root, inlined CSS — build it and open
`.output/chrome-mv3/block-page.js` from a throwaway HTML page that sets
`window.__focuser` to a `BlockContext` JSON string.

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

**Tailwind's source paths are declared, not detected.** `assets/tailwind.css`
names `../components`, `../entrypoints` and `../lib` with `@source`. Automatic
detection is rooted at the build root, which differs between the WXT build and
the preview — so without this the preview rendered every component with no CSS
at all while the real build was fine. A preview that silently disagrees with
the build is worse than no preview.

**The replaced `<body>` gets its margin zeroed explicitly.** It is created
fresh by the block script, so it carries the UA's 8px default, which puts a
scrollbar on every block page. The preview's own HTML zeroes it already, so
this is invisible there.

**A wildcard is matched against the host and the full URL, and `*.` includes
the apex.** `*.reddit.com` covers `reddit.com` itself, because that is what
people mean when they write it, and a pattern that quietly skipped the apex
looked broken. `focuser_common::host::wildcard_matches` is the same rule on the
Rust side; if the two ever disagree, a rule silently misses.

**Exceptions can be wildcards too.** `allowed_wildcards` had been served by the
app since the protocol was written and dropped on the floor here, so a wildcard
exception released nothing. `isAllowed` checks them now.

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

**Submitting is manual, on purpose.** Nothing in CI uploads to a store.
`.github/workflows/package-extension.yml` builds the zips on a `v*` tag, or on
demand from the Actions tab, and attaches them to the run as `extension-zips`.
Download them from there and upload them yourself.

It refuses to package anything that fails `tsc` or the tests first, because a
store review takes days and a broken build should cost seconds.

The workflow used to submit to both stores on every tag. That published a
version before anyone had looked at it, so it was cut back to packaging. If it
is ever restored, note that `.output` is a dot directory: `upload-artifact`
treats everything inside it as hidden and silently uploads nothing without
`include-hidden-files: true`.

### Getting set up locally

`publish-extension init` cannot bootstrap from nothing. In 5.1.0 it fabricates
a zip for every store before prompting, so validation demands every store's
credentials up front, including the Chrome refresh token the walkthrough exists
to generate. Starting from a filled-in file avoids that:

```bash
cd extension
cp .env.submit.example .env.submit
```

Fill in `CHROME_CLIENT_ID`, `CHROME_CLIENT_SECRET`, `FIREFOX_JWT_ISSUER` and
`FIREFOX_JWT_SECRET`. Leave the Edge and Opera placeholders alone; they are
only there to satisfy that validation, and nothing is submitted to a store
without a zip for it.

Then let the walkthrough do the OAuth exchange:

```bash
npx publish-extension init
```

Choose Chrome, answer yes to "Generate new refresh token?", approve in the
browser, and paste the code back. It writes `CHROME_REFRESH_TOKEN` into
`.env.submit`, which is gitignored.

Keep the file for local submissions. It does not belong in repository secrets
any more, since CI no longer submits anything.

Then locally:

```bash
npm run zip && npm run zip:firefox
npm run submit:dry   # checks credentials, uploads nothing
npm run submit       # the real thing
```

Firefox always gets the sources zip alongside the build. AMO rejects bundled
code without the source to rebuild it from, and everything here is bundled by
Vite. Chrome does not ask for it.

The Firefox build is **MV3**, matching the published AMO listing. WXT defaults
Firefox to MV2; `manifestVersion: 3` in `wxt.config.ts` overrides that, and
dropping it would be a regression for existing users.
