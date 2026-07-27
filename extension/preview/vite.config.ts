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
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": resolve(__dirname, "..") },
  },
  server: { port: 5199, strictPort: true },
});
