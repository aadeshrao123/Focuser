import { defineConfig } from "vitest/config";
import { WxtVitest } from "wxt/testing/vitest-plugin";

/**
 * `WxtVitest` polyfills the extension APIs with fake-browser, applies WXT's
 * aliases, and sets up auto-imports — so tests import from `#imports` and
 * `browser.*` the same way the real entrypoints do.
 */
export default defineConfig({
  plugins: [WxtVitest()],
  test: {
    environment: "jsdom",
    globals: true,
    restoreMocks: true,
  },
});
