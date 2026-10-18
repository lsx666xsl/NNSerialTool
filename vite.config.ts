import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// 本文件由 tsconfig.node.json 单独编译；不新增 @types/node 依赖，
// 用局部声明拿到 process（TypeScript 会优先使用模块内的同名声明）
declare const process: { env: Record<string, string | undefined> };

// https://vite.dev/config/
export default defineConfig(async () => {
  // 旧 macOS WKWebView（Safari 13 基线）不支持较新的 JS 语法；
  // Windows 侧 WebView2 是 Chromium，可用 chrome105 基线，产物更小更快
  const target = process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13";
  const host = process.env.TAURI_DEV_HOST;

  return {
    plugins: [vue()],

    build: {
      target,
    },

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
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
        // 3. tell Vite to ignore watching `src-tauri`
        ignored: ["**/src-tauri/**"],
      },
    },
  };
});
