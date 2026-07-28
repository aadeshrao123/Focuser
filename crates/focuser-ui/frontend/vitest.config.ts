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
    },
  }),
);
