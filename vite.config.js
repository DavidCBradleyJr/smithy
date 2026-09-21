import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// The webview keeps cached modules across dev-server restarts. When it
// revalidates one, Vite answers 304 without compiling the component, and
// vite-plugin-svelte then can't serve that component's CSS ("failed to load
// virtual css module"), leaving the page unstyled. Dropping the conditional
// request headers forces a fresh compile on every load.
const noConditionalRequests = {
  name: "smithy:no-conditional-requests",
  apply: "serve",
  configureServer(server) {
    server.middlewares.use((req, _res, next) => {
      delete req.headers["if-none-match"];
      delete req.headers["if-modified-since"];
      next();
    });
  },
};

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [noConditionalRequests, sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
