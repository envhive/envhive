<script setup lang="ts">
// TopBar —— 顶栏内容：当前页面标题 + 语言切换 + 全局状态标签
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { NTag, NIcon, NButton, NDropdown } from "naive-ui";
import { LanguageOutline } from "@vicons/ionicons5";
import { useApp } from "../store";
import { NAV_ITEMS } from "../types";
import { LOCALES, currentLocale, setLocale } from "../i18n";
import type { LocaleKey } from "../i18n";

const app = useApp();
const { t } = useI18n();

// 当前页标题（从导航配置反查；t() 依赖 locale，语言切换后自动刷新）
const pageTitle = computed(() => {
  const item = NAV_ITEMS.find((i) => i.key === app.page);
  return item ? { label: t(item.i18nKey), icon: item.icon } : { label: "", icon: null };
});

// 语言切换：label 用各语言自身写法，不随当前语言翻译
const langOptions = LOCALES.map((l) => ({ label: l.label, key: l.key }));
const currentLangLabel = computed(
  () => LOCALES.find((l) => l.key === currentLocale.value)?.label ?? ""
);
function onLangSelect(key: string | number) {
  setLocale(key as LocaleKey);
}
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
        ⚠ {{ t("topbar.storageReadonly") }}
      </n-tag>

      <!-- 语言切换 -->
      <n-dropdown trigger="click" :options="langOptions" @select="onLangSelect">
        <n-button quaternary size="small">
          <template #icon>
            <n-icon><LanguageOutline /></n-icon>
          </template>
          {{ currentLangLabel }}
        </n-button>
      </n-dropdown>
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
