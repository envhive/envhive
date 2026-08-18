<script setup lang="ts">
//工具管理 —— 安装 / 切换 / 卸载完整工作台，发行商×版本两级选择
import { ref, computed } from "vue";
import { NInput, NSelect, NAlert, NCard } from "naive-ui";
import { useApp } from "../store";
import ToolCard from "../components/ToolCard.vue";
import InstallToolModal from "../components/InstallToolModal.vue";
import EmptyState from "../components/EmptyState.vue";
import type { ToolInfo } from "../types";

const app = useApp();
const filter = ref("");
const cat = ref("全部");
// 安装弹窗目标：非空即打开（传给 InstallToolModal 渲染）
const installTarget = ref<ToolInfo | null>(null);

const categories = computed(() => ["全部", ...new Set(app.tools.map((s) => s.category))]);
const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase();
  return app.tools.filter(
    (s) =>
      (cat.value === "全部" || s.category === cat.value) &&
      (!q || s.name.toLowerCase().includes(q) || s.display.toLowerCase().includes(q))
  );
});

const catOptions = computed(() => categories.value.map((c) => ({ label: c, value: c })));

const installedCount = computed(() => app.tools.filter((s) => s.installed.length > 0).length);
</script>

<template>
  <section class="section">
    <n-card size="small" :title="`工具列表（${filtered.length}）`" class="section-card" :bordered="true">

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
          placeholder="搜索工具…"
          clearable
          style="width: 220px"
        />
        <span class="mono muted">
          已装 {{ installedCount }}
        </span>
      </div>

      <!-- 预览模式横幅：仅在后端确实未连接且无数据时展示；后端已连接但工具全被禁用时走下方空状态 -->
      <n-alert v-if="app.tools.length === 0 && !app.backend" type="warning" :bordered="false" class="preview-banner">
        预览模式：仅展示 UI，未连接 Rust 后端。
      </n-alert>

      <!-- 后端已连接但没有任何工具（全部被禁用 / 卸载） -->
      <EmptyState
        v-else-if="app.tools.length === 0"
        hint="所有工具均已被禁用或卸载，可前往插件市场重新安装。"
        action-label="前往插件市场"
        @action="app.page = 'plugins'"
      />

      <EmptyState
        v-else-if="filtered.length === 0"
        hint="尝试调整筛选条件或搜索关键词。"
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
