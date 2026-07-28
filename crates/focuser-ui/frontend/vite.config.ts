import path from "node:path";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Set by `tauri dev --host` for mobile/LAN testing; unset for normal desktop dev.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // Messages compile to one function each, so an unused string is dropped by
    // tree-shaking and a missing key is a build error rather than a silent
    // fallback to English.
    paraglideVitePlugin({
      project: "./project.inlang",
      outdir: "./src/paraglide",
      emitTsDeclarations: true,
      // One module per message, so Vite can drop the ones nobody imports.
      // Must match the `paraglide` script in package.json or the two produce
      // different trees depending on which ran last.
      outputStructure: "message-modules",
      // localStorage first so a reload keeps the chosen language without a
      // flash of English, then the base locale. Nothing is sniffed from the
      // URL or Accept-Language: the app owns the choice and the Settings page
      // is the only thing that makes it.
      strategy: ["localStorage", "baseLocale"],
    }),
  ],

  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
  },

  // Tauri prints Rust compile errors to the same terminal — don't wipe them.
  clearScreen: false,

  server: {
    // Fixed port: tauri.conf.json's devUrl must match exactly.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    // Watching Rust output would trigger endless reloads during cargo builds.
    watch: { ignored: ["**/src/**/*.rs", "**/target/**", "**/gen/**"] },
  },

  // Tauri injects TAURI_ENV_* and expects them readable from the frontend.
  envPrefix: ["VITE_", "TAURI_ENV_*"],

  build: {
    // The webview version is known per platform, so we can target it directly
    // rather than shipping downlevel output.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: process.env.TAURI_ENV_DEBUG ? false : "oxc",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
