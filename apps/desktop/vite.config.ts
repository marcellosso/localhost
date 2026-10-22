import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Tauri loads the dev server at a fixed port, and its webview (WebKitGTK on
// Linux) is the build target.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "safari16", outDir: "dist" },
  test: { environment: "jsdom", setupFiles: ["src/test/setup.ts"] },
});
