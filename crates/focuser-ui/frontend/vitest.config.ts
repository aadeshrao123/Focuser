import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

// Kept separate from vite.config.ts: Vite 8's `defineConfig` does not accept a
// `test` key, so co-locating them fails typecheck even though it runs.
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      globals: true,
      setupFiles: ["./src/test/setup.ts"],
    },
  }),
);
