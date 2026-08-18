<script setup lang="ts">
// App.vue —— 应用壳：Naive UI n-layout 整体布局
//   左侧 n-layout-sider（菜单） + 右侧 n-layout（header + content）
import { onMounted, onUnmounted, computed } from "vue";
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  NLayout,
  NLayoutHeader,
  NLayoutContent,
  zhCN,
  dateZhCN,
} from "naive-ui";
import type { GlobalThemeOverrides } from "naive-ui";
import { useApp, store, initEvents } from "./store";
import Sidebar from "./components/Sidebar.vue";
import TopBar from "./components/TopBar.vue";
import QueueDrawer from "./components/QueueDrawer.vue";
import HomePage from "./pages/HomePage.vue";
import ToolsPage from "./pages/ToolsPage.vue";
import NetworkPage from "./pages/NetworkPage.vue";
import PluginsPage from "./pages/PluginsPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import StatsPage from "./pages/StatsPage.vue";
import LogsPage from "./pages/LogsPage.vue";
import AboutPage from "./pages/AboutPage.vue";

const app = useApp();

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#534ab7",
    primaryColorHover: "#6b62c9",
    primaryColorPressed: "#453c9e",
    primaryColorSuppl: "#534ab7",
    borderRadius: "8px",
  },
  Card: {
    borderRadius: "12px",
  },
  Button: {
    borderRadiusMedium: "8px",
    borderRadiusSmall: "6px",
  },
};

const currentPage = computed(() => app.page);

let cleanupEvents: (() => void) | null = null;
onMounted(() => {
  void store.loadAll();
  cleanupEvents = initEvents();
});
onUnmounted(() => cleanupEvents?.());
</script>

<template>
  <n-config-provider :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
    <n-message-provider>
      <n-dialog-provider>
        <n-layout position="absolute" class="app-shell">
          <n-layout has-sider position="absolute">
            <Sidebar />
            <n-layout>
              <n-layout-header bordered class="app-header">
                <TopBar />
              </n-layout-header>

              <n-layout-content class="app-content" content-style="padding: 24px 28px 56px;">
                <div v-if="!app.appLoaded" class="app-loading" role="status" aria-live="polite">
                  <div class="loading-card">
                    <div class="spinner" aria-hidden="true">
                      <span></span><span></span><span></span><span></span>
                    </div>
                    <div class="loading-title">蜂巢 EnvHive 正在启动</div>
                    <div class="loading-sub">正在加载环境信息、镜像配置与队列状态…</div>
                  </div>
                </div>

                <template v-else>
                  <HomePage v-if="currentPage === 'home'" />
                  <ToolsPage v-else-if="currentPage === 'tools'" />
                  <NetworkPage v-else-if="currentPage === 'network'" />
                  <PluginsPage v-else-if="currentPage === 'plugins'" />
                  <SettingsPage v-else-if="currentPage === 'settings'" />
                  <StatsPage v-else-if="currentPage === 'stats'" />
                  <LogsPage v-else-if="currentPage === 'logs'" />
                  <AboutPage v-else-if="currentPage === 'about'" />
                </template>
              </n-layout-content>
            </n-layout>
          </n-layout>

          <QueueDrawer />
        </n-layout>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style scoped>
.app-shell {
  background: #f6f7f9;
}
.app-header {
  display: flex;
  align-items: center;
  padding: 0 24px;
  min-height: 56px;
  background: #fff;
}
.app-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: calc(100vh - 56px - 80px); /* 减去顶部栏 + 底部留白 */
  padding: 24px;
}
.loading-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 28px 36px;
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 14px;
  box-shadow: 0 4px 16px rgba(83, 74, 183, 0.06);
  min-width: 280px;
}
.loading-title {
  font-size: 15px;
  font-weight: 600;
  color: #1f2329;
  letter-spacing: 0.01em;
}
.loading-sub {
  font-size: 12.5px;
  color: #6b7280;
  text-align: center;
  max-width: 320px;
  line-height: 1.5;
}
.spinner {
  display: inline-block;
  position: relative;
  width: 36px;
  height: 36px;
}
.spinner span {
  display: block;
  position: absolute;
  width: 8px;
  height: 8px;
  background: #534ab7;
  border-radius: 50%;
  animation: spinner-orb 1.2s cubic-bezier(0.5, 0, 0.5, 1) infinite;
}
.spinner span:nth-child(1) { top: 0; left: 14px; animation-delay: 0s; }
.spinner span:nth-child(2) { top: 4px; right: 2px; animation-delay: 0.15s; }
.spinner span:nth-child(3) { bottom: 2px; right: 4px; animation-delay: 0.3s; }
.spinner span:nth-child(4) { bottom: 4px; left: 2px; animation-delay: 0.45s; }
@keyframes spinner-orb {
  0%, 100% { opacity: 0.25; transform: scale(0.8); }
  50%      { opacity: 1;    transform: scale(1); }
}
</style>
