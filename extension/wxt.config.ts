import { existsSync } from "node:fs";
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "wxt";

/**
 * The starter category lists live with the desktop app's frontend and are the
 * one source of truth. Copying them at build time beats keeping a second copy
 * here that quietly drifts — which is exactly what happened before, when the
 * old build never copied the file at all and every block page fell back to the
 * generic message.
 */
const PREMADE_LISTS = resolve(
  __dirname,
  "../crates/focuser-ui/frontend/public/premade-lists.json",
);

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

  hooks: {
    "build:publicAssets": (_wxt, assets) => {
      if (!existsSync(PREMADE_LISTS)) {
        throw new Error(
          `Starter lists missing at ${PREMADE_LISTS}. The block page cannot label categories without them.`,
        );
      }
      assets.push({
        absoluteSrc: PREMADE_LISTS,
        relativeDest: "premade-lists.json",
      });
    },
  },
});
