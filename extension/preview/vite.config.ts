import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/**
 * Standalone Vite config for the UI preview. Separate from WXT's own build —
 * this one produces an ordinary web page, not an extension.
 */
export default defineConfig({
  root: __dirname,
  // The extension's own static files, so /icons/icon128.png on the welcome page
  // resolves instead of rendering a broken-image box.
  publicDir: resolve(__dirname, "../public"),
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(__dirname, ".."),
      // WXT generates this one during `wxt prepare`, and only its own build
      // knows the alias. Without it the preview will not start at all.
      "#i18n": resolve(__dirname, "../.wxt/i18n/index.ts"),
    },
  },
  server: { port: 5199, strictPort: true },
});
