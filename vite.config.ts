import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },

  // Vite 6: allow pre-compiled CJS modules from tauri deps.
  optimizeDeps: {
    esbuildOptions: {
      target: "es2021",
    },
  },

  build: {
    target: "es2021",
    outDir: "dist",
    // Tauri builds the frontend statically; no need for chunk warnings.
    chunkSizeWarningLimit: 1000,
  },

  // Tauri expects a fixed dev port.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Don't trigger reloads when Rust recompiles.
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
});
