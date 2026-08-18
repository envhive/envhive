import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { readFileSync } from "node:fs";

const host = process.env.TAURI_DEV_HOST;

// 读取 package.json 版本号，与 src-tauri/tauri.conf.json 保持一致
const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf-8"));

// 构建时刻注入：__APP_VERSION__（版本号）、__BUILD_TIME__（打包时间 ISO）
const appVersion = String(pkg.version ?? "0.0.0");
const buildTime = new Date().toISOString();

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    __BUILD_TIME__: JSON.stringify(buildTime),
  },
  build: {
    // 关闭 vite 自动清空 dist：safe-delete shim 会拦截删除，Windows 上构建报错
    emptyOutDir: false,
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
