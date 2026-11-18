import { defineConfig } from "vite";
import { resolve } from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        picker: resolve(__dirname, "picker.html"),
        settings: resolve(__dirname, "settings.html"),
        systemConfig: resolve(__dirname, "system-config.html"),
        records: resolve(__dirname, "records.html"),
        chrome: resolve(__dirname, "chrome.html"),
        highlight: resolve(__dirname, "highlight.html"),
        grip: resolve(__dirname, "grip.html"),
        empty: resolve(__dirname, "empty.html"),
      },
    },
  },
}));
