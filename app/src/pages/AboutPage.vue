<script setup lang="ts">
// 关于 —— 品牌愿景 / 技术架构 / 应用更新 / 开源仓库
import {NTag, NIcon} from "naive-ui";
import {LogoGithub, GitBranchOutline, ShieldCheckmarkOutline} from "@vicons/ionicons5";
import {openUrl} from "@tauri-apps/plugin-opener";
import hiveLogo from "../assets/icon.png";

// 版本 / 打包时间（由 vite.config.ts define 注入，与 package.json / tauri.conf.json 保持一致）
const appVersion = __APP_VERSION__;
const appName = "蜂巢 EnvHive";
const appTagline = "多语言运行时 · 环境配置中枢";

// 打包时间：ISO → "YYYY-MM-DD HH:mm:ss"
function formatBuildTime(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

const buildTimeText = formatBuildTime(__BUILD_TIME__);

// 开源仓库（display 为页面短展示，去掉 https:// 前缀让排版更紧凑）
const REPOS = [
  {name: "GitHub", url: "https://github.com/envhive/envhive", display: "github.com/envhive/envhive"},
  {name: "Gitee", url: "https://gitee.com/envhive/envhive", display: "gitee.com/envhive/envhive"},
];

// 开源协议（木兰宽松许可证 第2版，与仓库 LICENSE 一致）
const LICENSE = {
  name: "License",
  url: "https://license.coscl.org.cn/MulanPSL2",
  display: "Mulan PSL v2 · 木兰宽松许可证第2版",
};

async function openRepo(url: string) {
  try {
    await openUrl(url);
  } catch {
    // 浏览器环境（如 vite dev 直开）降级为新标签页
    window.open(url, "_blank");
  }
}

// 技术架构：三列等宽卡片
const TECH_ARCH = [
  {layer: "桌面框架", choice: "Tauri 2", detail: "Rust 后端，安装包约 10MB"},
  {layer: "前端", choice: "Vue 3 + TypeScript", detail: "Vite + Naive UI"},
  {layer: "后端", choice: "Rust (cargo)", detail: "工具全部由 Lua 插件驱动"},
];

// 检查更新：按钮与 checkUpdate 实现暂未启用（模板中已注释；如需恢复，参考 updater.ts 与 git 历史）
</script>

<template>
  <section class="about">
    <!-- ============ Hero：品牌与愿景 ============ -->
    <div class="hero">
      <div class="hero-main">
        <div class="hero-hive" aria-hidden="true">
          <img class="hive-logo-img" :src="hiveLogo" alt="" style="width: 50px"/>
        </div>
        <div class="hero-text">
          <h1 class="hero-title">
            {{ appName }}
          </h1>
          <p class="hero-tagline">{{ appTagline }} <n-tag size="small" round :bordered="false">v{{ appVersion }}</n-tag></p>
        </div>
      </div>

<!--      <n-button-->
<!--          class="hero-check"-->
<!--          size="small"-->
<!--          :loading="checking || updateStatus === 'downloading'"-->
<!--          :disabled="updateStatus === 'downloading'"-->
<!--          @click="checkUpdate"-->
<!--      >-->
<!--        <template #icon>-->
<!--          <n-icon :size="15">-->
<!--            <RefreshOutline/>-->
<!--          </n-icon>-->
<!--        </template>-->
<!--        {{-->
<!--          updateStatus === "downloading"-->
<!--              ? `正在更新 ${Math.round(updateProgress * 100)}%`-->
<!--              : "检查更新"-->
<!--        }}-->
<!--      </n-button>-->
    </div>

    <!-- ============ 技术架构 ============ -->
    <n-card size="small" title="技术架构" class="section-card" :bordered="true">
      <div class="arch-grid">
        <div v-for="item in TECH_ARCH" :key="item.layer" class="arch-card">
          <span class="arch-layer">{{ item.layer }}</span>
          <span class="arch-choice">{{ item.choice }}</span>
          <span class="arch-detail">{{ item.detail }}</span>
        </div>
      </div>
    </n-card>

    <!-- ============ 开源仓库 ============ -->
    <n-card size="small" title="开源仓库" class="section-card repo-card" :bordered="true">
      <div class="repo-list">
        <button
            v-for="repo in REPOS"
            :key="repo.name"
            type="button"
            class="repo-item"
            @click="openRepo(repo.url)"
        >
          <n-icon class="repo-icon" :size="18">
            <LogoGithub v-if="repo.name === 'GitHub'"/>
            <GitBranchOutline v-else/>
          </n-icon>
          <span class="repo-name">{{ repo.name }}</span>
          <span class="repo-url">{{ repo.display }}</span>
        </button>
        <button type="button" class="repo-item" @click="openRepo(LICENSE.url)">
          <n-icon class="repo-icon" :size="18">
            <ShieldCheckmarkOutline/>
          </n-icon>
          <span class="repo-name">{{ LICENSE.name }}</span>
          <span class="repo-url">{{ LICENSE.display }}</span>
        </button>
      </div>
      <p class="muted repo-hint">点击打开仓库 / 协议原文，欢迎 Star / Issue / PR，一起共建蜂巢生态。</p>
    </n-card>

    <!-- ============ 页脚 ============ -->
    <p class="muted footer-note">
      {{ appName }} · v{{ appVersion }} · 打包于 {{ buildTimeText }}
    </p>
  </section>
</template>

<style scoped>
.about {
  max-width: 880px;
}

.section-card {
  margin-bottom: 18px;
}

.section-card :deep(.n-card-header) {
  padding: 16px 20px 12px;
}

.section-card :deep(.n-card-header__title) {
  font-size: 15px;
  font-weight: 600;
}

.section-card :deep(.n-card__content) {
  padding: 4px 20px 18px;
}

.muted {
  color: #6b7280;
  font-size: 13px;
}

/* ---------- Hero ---------- */
.hero {
  background: linear-gradient(135deg, #534ab7 0%, #6b5fd4 55%, #8b7fe8 100%);
  border-radius: 14px;
  padding: 28px 32px;
  margin-bottom: 16px;
  color: #fff;
  position: relative;
  overflow: hidden;
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.hero::after {
  content: "";
  position: absolute;
  right: -60px;
  top: -60px;
  width: 220px;
  height: 220px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.08);
}

.hero-main {
  display: flex;
  gap: 18px;
  align-items: center;
  min-width: 0;
}

.hero-hive {
  width: 52px;
  height: 52px;
  flex-shrink: 0;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.14);
  border: 1px solid rgba(255, 255, 255, 0.16);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 8px;
}

.hero-text {
  min-width: 0;
}

.hero-title {
  font-size: 24px;
  font-weight: 700;
  margin: 0 0 4px;
  letter-spacing: 0.5px;
  display: flex;
  align-items: center;
  gap: 8px;
}

.hero-title :deep(.n-tag) {
  --n-color: rgba(255, 255, 255, 0.18);
  --n-text-color: #ffffff;
  font-weight: 500;
}

.hero-tagline {
  font-size: 15px;
  opacity: 0.92;
  margin: 0 0 6px;
}

.hero-check {
  flex-shrink: 0;
  z-index: 1;
  background: rgba(255, 255, 255, 0.16) !important;
  color: #fff !important;
  border-color: rgba(255, 255, 255, 0.35) !important;
}

.hero-check:hover {
  background: rgba(255, 255, 255, 0.28) !important;
}

.arch-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
}

.arch-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px 16px;
  border: 1px solid #ececf1;
  border-radius: 10px;
  background: #fafafc;
  transition: border-color 0.2s;
}

.arch-card:hover {
  border-color: #c8c3ee;
}

.arch-layer {
  display: inline-block;
  align-self: flex-start;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.4px;
  color: #534ab7;
  background: #efeefb;
  padding: 2px 8px;
  border-radius: 4px;
}

.arch-choice {
  font-size: 15px;
  font-weight: 600;
  color: #1f2329;
  margin-top: 2px;
}

.arch-detail {
  font-size: 12px;
  color: #6b7280;
  line-height: 1.6;
}

.repo-card {
  background: #f8fafc;
}

.repo-card :deep(.n-card-header__title) {
  color: #155e75;
}

.repo-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 18px;
}

.repo-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 12px 16px;
  border: 1px solid #e2e8f0;
  border-radius: 10px;
  background: #fff;
  cursor: pointer;
  text-align: left;
  font: inherit;
  color: inherit;
  transition: border-color 0.2s,
  box-shadow 0.2s,
  transform 0.1s;
}

.repo-item:hover {
  border-color: #534ab7;
  box-shadow: 0 2px 10px rgba(83, 74, 183, 0.12);
}

.repo-item:active {
  transform: translateY(1px);
}

.repo-icon {
  color: #534ab7;
  flex-shrink: 0;
}

.repo-name {
  font-size: 13px;
  font-weight: 600;
  color: #1f2329;
  flex-shrink: 0;
  min-width: 56px;
}

.repo-url {
  font-size: 12px;
  color: #6b7280;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.repo-hint {
  margin-top: 14px;
  color: #9ca3af;
}

/* ---------- 页脚 ---------- */
.footer-note {
  text-align: center;
  margin-top: 18px;
  padding: 14px 0 4px;
  font-size: 13px;
  color: #9ca3af;
}

.footer-note strong {
  color: #534ab7;
  font-weight: 600;
}
</style>
