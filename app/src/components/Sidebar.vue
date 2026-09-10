<script setup lang="ts">
// Sidebar —— 左侧导航栏（Naive UI n-layout-sider + n-menu，支持折叠）
import { ref, computed, h } from "vue";
import { useI18n } from "vue-i18n";
import { NLayoutSider, NMenu, NIcon } from "naive-ui";
import type { MenuOption } from "naive-ui";
import { useApp } from "../store";
import { NAV_ITEMS, type PageKey } from "../types";
// 品牌图标：src/assets/icon.png（由 src-tauri/icons/icon.png 复制，Vite 打包时处理为资源 URL）
import hiveLogo from "../assets/icon.png";

const app = useApp();
const { t } = useI18n();
const collapsed = ref(false);

// 平铺菜单项（无分组，icon 用 ionicons5 SVG 组件，经 NIcon 包裹以随菜单着色/调尺寸）
// label 走 i18n key，计算属性使其随语言切换即时刷新
const menuOptions = computed<MenuOption[]>(() =>
  NAV_ITEMS.map((item) => ({
    key: item.key,
    label: t(item.i18nKey),
    icon: () => h(NIcon, null, { default: () => h(item.icon) }),
  }))
);

const activeKey = computed(() => app.page);

function onSelect(key: string) {
  app.page = key as PageKey;
}
</script>

<template>
  <n-layout-sider
    bordered
    collapse-mode="width"
    :width="220"
    :collapsed-width="64"
    show-trigger
    v-model:collapsed="collapsed"
    class="app-sider"
  >
    <!-- 品牌区 -->
    <div class="sider-brand" :class="{ collapsed }">
      <div class="hive-logo" aria-hidden="true">
        <img class="hive-logo-img" :src="hiveLogo" alt="" />
      </div>
      <div v-show="!collapsed" class="sider-brand-text">
        <strong>蜂巢</strong>
        <span>EnvHive</span>
      </div>
    </div>

    <n-menu
      :options="menuOptions"
      :value="activeKey"
      :collapsed="collapsed"
      :collapsed-width="64"
      :icon-size="16"
      :collapsed-icon-size="20"
      :root-indent="14"
      :indent="18"
      accordion
      @update:value="onSelect"
    />
  </n-layout-sider>
</template>

<style scoped>
.app-sider {
  background: #fff;
}
.sider-brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 18px 16px 14px;
}
.sider-brand.collapsed {
  justify-content: center;
  padding-left: 8px;
  padding-right: 8px;
}
.hive-logo {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: 10px;
  background: #fffdf7;
  border: 1px solid #f0e6d0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 5px;
  box-shadow: 0 2px 8px rgba(94, 66, 30, 0.14);
}
.hive-logo-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  border-radius: 8px;
}
.sider-brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.2;
  min-width: 0;
}
.sider-brand-text strong {
  font-size: 17px;
  font-weight: 700;
  color: #1f2329;
}
.sider-brand-text span {
  font-size: 12px;
  color: #534ab7;
  font-weight: 600;
}
</style>
