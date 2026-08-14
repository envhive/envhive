/// <reference types="vite/client" />

// vite.config.ts define 注入的构建常量
declare const __APP_VERSION__: string; // 应用版本号（来自 package.json）
declare const __BUILD_TIME__: string; // 打包时间（ISO 8601）
