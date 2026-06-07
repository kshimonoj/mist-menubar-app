import { defineConfig } from "vite";

// Tauri expects a fixed port and a build that emits into dist/
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
