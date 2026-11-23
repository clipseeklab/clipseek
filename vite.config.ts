import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  // Keep native IPC dependencies separate from browser demos using mock aliases.
  cacheDir: "node_modules/.vite/client",
  plugins: [react()],
  build: {
    outDir: process.env.VITE_E2E === "true" ? "dist-e2e" : "dist",
    // macOS 12 ships a Safari 15-generation WKWebView. Do not emit the newer
    // baseline target used by current Vite releases.
    target: "safari15",
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@test": path.resolve(__dirname, "./tests/frontend"),
      ...(process.env.VITE_E2E === "true"
        ? { "@tauri-apps/plugin-dialog": path.resolve(__dirname, "tests/frontend/e2e/dialogs.ts") }
        : {}),
    },
  },

  test: {
    environment: "jsdom",
    // Bound concurrent jsdom trees on Windows without changing test isolation or deadlines.
    maxWorkers: process.platform === "win32" ? 2 : undefined,
    include: ["tests/frontend/**/*.test.{ts,tsx}"],
    setupFiles: ["./tests/frontend/setup.ts"],
    css: true,
    clearMocks: true,
    restoreMocks: true,
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "html", "lcov", "json-summary"],
      thresholds: { lines: 86.7, statements: 83.6, branches: 78.8, functions: 78.9 },
      reportsDirectory: "coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["tests/**", "src/**/*.d.ts", "src/main.tsx"],
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 14200,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 14201,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
