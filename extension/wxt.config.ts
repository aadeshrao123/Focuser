import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "wxt";

// `public/premade-lists.json` is a copy of the desktop app's. It used to be
// read from `../crates/...` at build time, which made this folder impossible
// to build on its own — and AMO requires a source archive a reviewer can build.
// `starter_lists_match` in focuser-ui fails if the two copies drift.

const ICONS = {
  16: "icons/icon16.png",
  32: "icons/icon32.png",
  48: "icons/icon48.png",
  96: "icons/icon96.png",
  128: "icons/icon128.png",
};

export default defineConfig({
  modules: ["@wxt-dev/module-react"],
  srcDir: ".",
  outDir: ".output",

  // WXT defaults Firefox to MV2. The published AMO listing is MV3 and has been
  // since Firefox 142, so dropping to MV2 would be a regression for existing
  // users rather than a compatibility win.
  manifestVersion: 3,

  manifest: ({ browser }) => ({
    name: "Focuser",
    description:
      "Blocks distracting sites so you stay focused. Enforces the block lists you set in the Focuser desktop app.",
    // Only what is actually used. `webRequest` and `nativeMessaging` were
    // declared before and never needed — both cost review scrutiny for nothing.
    permissions: ["tabs", "webNavigation", "scripting", "storage", "alarms"],
    host_permissions: ["<all_urls>"],
    icons: ICONS,
    action: { default_icon: ICONS },
    // The block page is injected into blocked tabs rather than navigated to,
    // so its script and the category data must be readable from any origin.
    web_accessible_resources: [
      {
        matches: ["<all_urls>"],
        resources: ["block-page.js", "premade-lists.json"],
      },
    ],
    // Chrome ignores unknown keys, but shipping Gecko settings in a Chrome
    // build is noise a store reviewer has to read past.
    ...(browser === "firefox"
      ? {
          browser_specific_settings: {
            gecko: {
              id: "focuser@focuser-app",
              strict_min_version: "142.0",
              data_collection_permissions: { required: ["none"] },
            },
          },
        }
      : {}),
  }),

  vite: () => ({
    plugins: [tailwindcss()],
  }),
});
