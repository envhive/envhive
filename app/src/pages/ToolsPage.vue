<script setup lang="ts">
//工具管理 —— 安装 / 切换 / 卸载完整工作台，发行商×版本两级选择
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { NInput, NSelect, NAlert, NCard } from "naive-ui";
import { useApp } from "../store";
import ToolCard from "../components/ToolCard.vue";
import InstallToolModal from "../components/InstallToolModal.vue";
import EmptyState from "../components/EmptyState.vue";
import type { ToolInfo } from "../types";

const app = useApp();
const { t } = useI18n();
const filter = ref("");
// 分类筛选："" 为「全部」哨兵值（渲染时走 i18n，避免把中文写进状态）
const cat = ref("");
// 安装弹窗目标：非空即打开（传给 InstallToolModal 渲染）
const installTarget = ref<ToolInfo | null>(null);

const categories = computed(() => ["", ...new Set(app.tools.map((s) => s.category))]);
const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase();
  return app.tools.filter(
    (s) =>
      (cat.value === "" || s.category === cat.value) &&
      (!q || s.name.toLowerCase().includes(q) || s.display.toLowerCase().includes(q))
  );
});

const catOptions = computed(() =>
  categories.value.map((c) => ({ label: c === "" ? t("tools.allCategories") : c, value: c }))
);

const installedCount = computed(() => app.tools.filter((s) => s.installed.length > 0).length);
</script>

<template>
  <section class="section">
    <n-card size="small" :title="t('tools.listTitle', { count: filtered.length })" class="section-card" :bordered="true">

      <!-- 顶部筛选条 -->
      <div class="tool-toolbar">
        <n-select
          size="small"
          class="tool-cat-select"
          :value="cat"
          :options="catOptions"
          style="width: 130px"
          @update:value="(v: string) => (cat = v)"
        />
        <n-input
          v-model:value="filter"
          size="small"
          :placeholder="t('tools.searchPlaceholder')"
          clearable
          style="width: 220px"
        />
        <span class="mono muted">
          {{ t("tools.installedCount", { count: installedCount }) }}
        </span>
      </div>

      <!-- 预览模式横幅：仅在后端确实未连接且无数据时展示；后端已连接但工具全被禁用时走下方空状态 -->
      <n-alert v-if="app.tools.length === 0 && !app.backend" type="warning" :bordered="false" class="preview-banner">
        {{ t("common.previewBanner") }}
      </n-alert>

      <!-- 后端已连接但没有任何工具（全部被禁用 / 卸载） -->
      <EmptyState
        v-else-if="app.tools.length === 0"
        :hint="t('tools.emptyAllHint')"
        :action-label="t('tools.emptyAllAction')"
        @action="app.page = 'plugins'"
      />

      <EmptyState
        v-else-if="filtered.length === 0"
        :hint="t('tools.emptyFilterHint')"
      />

      <div v-else class="tool-list">
        <ToolCard v-for="(s, i) in filtered" :key="s.name" :tool="s" :index="i" @install-more="installTarget = $event" />
      </div>
    </n-card>

    <!-- 安装新版本弹窗（低频操作从卡片剥离，含发行商 / 加速镜像选择） -->
    <InstallToolModal
      :show="!!installTarget"
      :tool="installTarget"
      @update:show="(v: boolean) => { if (!v) installTarget = null }"
    />
  </section>
</template>

<style scoped>
.tool-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.section-card {
  margin-bottom: 16px;
}
.section-card :deep(.n-card-header) {
  padding-bottom: 10px;
}
.preview-banner {
  margin-bottom: 12px;
}
.tool-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
</style>
