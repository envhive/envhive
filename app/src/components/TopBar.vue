<script setup lang="ts">
// TopBar —— 顶栏内容：当前页面标题 + 全局状态标签
import { computed } from "vue";
import { NTag, NIcon } from "naive-ui";
import { useApp } from "../store";
import { NAV_ITEMS } from "../types";

const app = useApp();

// 当前页标题（从导航配置反查）
const pageTitle = computed(() => {
  const item = NAV_ITEMS.find((i) => i.key === app.page);
  return item ? { label: item.label, icon: item.icon } : { label: "", icon: null };
});
</script>

<template>
  <div class="topbar">
    <div class="topbar-title">
      <n-icon v-if="pageTitle.icon" :size="18">
        <component :is="pageTitle.icon" />
      </n-icon>
      <span class="topbar-label">{{ pageTitle.label }}</span>
    </div>

    <div class="topbar-right">
      <!-- 环境徽标：仅异常时显示（存储不可写），常态隐藏 -->
      <n-tag v-if="app.bootstrap && !app.bootstrap.configWritable" type="error" round size="small">
        ⚠ 存储不可写
      </n-tag>
    </div>
  </div>
</template>

<style scoped>
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex: 1;
  min-width: 0;
  gap: 16px;
}
.topbar-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.topbar-label {
  font-size: 16px;
  font-weight: 600;
  color: #1f2329;
}
.topbar-right {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
</style>
