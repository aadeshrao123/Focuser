import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

// Kept separate from vite.config.ts: Vite 8's `defineConfig` does not accept a
// `test` key, so co-locating them fails typecheck even though it runs.
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      // jsdom defaults to `about:blank`, which is an opaque origin, and reading
      // `localStorage` from one throws. Paraglide reads it to resolve the
      // locale, so every render would fail without a real URL here.
      environmentOptions: { jsdom: { url: "http://localhost/" } },
      globals: true,
      setupFiles: ["./src/test/setup.ts"],
      // Role queries on the Schedule page walk a grid of 168 buttons. One test
      // that takes two seconds here took over five on a CI runner, which is
      // the default limit. It also has to be longer than `asyncUtilTimeout`.
      testTimeout: 20_000,
    },
  }),
);
