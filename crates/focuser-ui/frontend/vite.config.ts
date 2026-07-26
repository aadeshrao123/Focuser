import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Set by `tauri dev --host` for mobile/LAN testing; unset for normal desktop dev.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],

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
