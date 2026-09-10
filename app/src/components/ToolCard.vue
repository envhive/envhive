<script setup lang="ts">
// ToolCard —— 纯管理视图：左栏 =工具身份 + 当前版本摘要；右区 = 已安装版本紧凑表格（切换/卸载/取消使用）
// 安装新版本 → 通过「安装」按钮打开 InstallToolModal 弹窗（低频操作不占卡片空间）
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { NButton, NTag } from "naive-ui";
import { useApp, store } from "../store";
import type { ToolInfo } from "../types";
import { compareVersions } from "../types";
import ToolIcon from "./ToolIcon.vue";

const props = defineProps<{ tool: ToolInfo; index: number }>();
const emit = defineEmits<{ (e: "install-more", tool: ToolInfo): void }>();

const app = useApp();
const { t } = useI18n();
const busyTool = computed(() => app.busy === props.tool.name);

// 已安装：当前版本置顶，其余 semver 降序
const installedSorted = computed(() => {
  const list = [...props.tool.installed].sort(compareVersions).reverse();
  if (props.tool.current && list.includes(props.tool.current)) {
    return [props.tool.current, ...list.filter((v) => v !== props.tool.current)];
  }
  return list;
});

// 长列表折叠：当前版本恒在首行，折叠 5 行后仍可见
const COLLAPSE_THRESHOLD = 5;
const expanded = ref(false);
const hiddenCount = computed(() => Math.max(0, installedSorted.value.length - COLLAPSE_THRESHOLD));
const visibleVersions = computed(() =>
  expanded.value || hiddenCount.value === 0
    ? installedSorted.value
    : installedSorted.value.slice(0, COLLAPSE_THRESHOLD)
);
</script>

<template>
  <div class="tool-card">
    <!-- ==================== 左栏：身份 + 当前版本摘要 ==================== -->
    <div class="id-panel">
      <div class="inst-head">
        <ToolIcon :icon="tool.icon" :index="index" :size="20" :name="tool.display" />
        <span class="card-name">{{ tool.display }}</span>
        <span class="mono muted card-cat">{{ tool.category }}</span>
      </div>

      <div class="card-meta">
        <a :href="tool.homepage" target="_blank" rel="noreferrer">
          {{ tool.homepage.replace("https://", "") }}
        </a>
        <span v-if="tool.binPath" class="mono path-ok" :title="tool.binPath">PATH ✓</span>
        <span v-else-if="tool.installed.length" class="mono path-bad" :title="t('toolCard.pathMissing')">PATH ✗</span>
      </div>

      <p v-if="tool.current" class="current-line">
        <span class="current-lbl">{{ t("toolCard.current") }}</span>
        <n-tag size="small" type="primary" :bordered="false" round class="current-tag">{{ tool.current }}</n-tag>
      </p>
      <p v-else class="hint">{{ t("toolCard.noGlobalVersion") }}</p>
    </div>

    <!-- ==================== 右区：已安装版本紧凑表格 + 折叠 ==================== -->
    <div class="mgmt-panel">
      <div class="mgmt-head">
        <span class="mgmt-title">
          {{ t("toolCard.installedVersions") }} <span class="count-pill">{{ tool.installed.length }}</span>
        </span>
        <n-button size="small" secondary type="primary" :disabled="busyTool" @click="emit('install-more', tool)">
          {{ t("common.install") }}
        </n-button>
      </div>

      <div v-if="tool.installed.length === 0" class="empty-vers">
        <p class="hint">{{ t("toolCard.emptyVersions") }}</p>
      </div>
      <div v-else class="ver-list">
        <!-- 表头 -->
        <div class="ver-row ver-head">
          <span />
          <div class="ver-cell">
            <span class="col-ver">{{ t("toolCard.colVersion") }}</span>
            <span class="col-ops-head">{{ t("toolCard.colOps") }}</span>
          </div>
        </div>

        <div
          v-for="v in visibleVersions"
          :key="v"
          class="ver-row"
          :class="{ current: tool.current === v }"
        >
          <span class="ver-dot" :class="{ cur: tool.current === v }" />
          <div class="ver-cell">
            <span class="ver-name-wrap">
              <span class="mono ver-name">
                {{ v }}<span v-if="v.includes('.fx-')" class="ver-fx"> FX</span>
              </span>
              <n-tag v-if="tool.current === v" size="tiny" type="primary" :bordered="false" round class="ver-tag">
                {{ t("toolCard.current") }}
              </n-tag>
            </span>
            <div class="ver-ops">
              <template v-if="tool.current !== v">
                <n-button
                  size="tiny"
                  secondary
                  :loading="busyTool"
                  :disabled="busyTool"
                  :title="t('toolCard.switchTitle')"
                  @click="store.switchVersion(tool, v)"
                >
                  {{ t("toolCard.switch") }}
                </n-button>
                <n-button
                  size="tiny"
                  tertiary
                  type="error"
                  :disabled="busyTool"
                  :title="t('toolCard.uninstallTitle')"
                  @click="store.uninstallVersion(tool, v)"
                >
                  {{ t("toolCard.uninstall") }}
                </n-button>
              </template>
              <n-button
                v-else
                size="tiny"
                tertiary
                type="warning"
                :disabled="busyTool"
                :title="t('toolCard.unuseTitle')"
                @click="store.unuseGlobal(tool)"
              >
                {{ t("toolCard.unuse") }}
              </n-button>
            </div>
          </div>
        </div>

        <!-- 折叠展开 -->
        <button
          v-if="hiddenCount > 0 && !expanded"
          type="button"
          class="ver-expand"
          @click="expanded = true"
        >
          {{ t("toolCard.expandMore", { count: hiddenCount }) }}
        </button>
        <button
          v-else-if="hiddenCount > 0 && expanded"
          type="button"
          class="ver-expand"
          @click="expanded = false"
        >
          {{ t("common.collapse") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tool-card {
  display: flex;
  border: 1px solid #eef0f3;
  border-radius: 12px;
  background: #fff;
  overflow: hidden;
}
.id-panel {
  width: 300px;
  flex-shrink: 0;
  background: #fafbfc;
  border-right: 1px solid #eef0f3;
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
}
.mgmt-panel {
  flex: 1;
  min-width: 0;
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
}
.inst-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}
.card-name {
  font-weight: 700;
  font-size: 15px;
}
.card-cat {
  font-size: 10px;
}
.card-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 11px;
  color: #6b7280;
  margin-bottom: 10px;
}
.card-meta a {
  color: #534ab7;
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.path-ok {
  color: #16a34a;
}
.path-bad {
  color: #dc2626;
}
.current-line {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
}
.current-lbl {
  font-size: 11px;
  color: #9ca3af;
}
.current-tag {
  font-family: var(--font-mono, "Cascadia Code", Consolas, monospace);
}
.hint {
  font-size: 11px;
  color: #9ca3af;
  margin: 0;
}
.mgmt-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}
.mgmt-title {
  font-size: 13px;
  font-weight: 500;
  color: #374151;
}
.count-pill {
  display: inline-block;
  min-width: 18px;
  margin-left: 2px;
  padding: 0 6px;
  font-size: 11px;
  line-height: 18px;
  color: #534ab7;
  background: #f3f4ff;
  border-radius: 10px;
  text-align: center;
}
.empty-vers {
  padding: 8px 0;
}
.ver-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
/* 两列网格：状态点 + 内容区；内容区内部 flex space-between 让版本和操作紧贴两侧 */
.ver-row {
  display: grid;
  grid-template-columns: 12px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: 6px;
  border: 1px solid transparent;
}
.ver-row.current {
  background: #f3f4ff;
  border-color: #dcd9f6;
}
.ver-head {
  padding: 2px 8px 4px;
  border: none;
  background: transparent;
}
.ver-cell {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}
.ver-name-wrap {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  overflow: hidden;
}
.col-ver,
.col-ops-head {
  font-size: 11px;
  color: #9ca3af;
}
.ver-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #9fe1cb;
  flex-shrink: 0;
}
.ver-dot.cur {
  background: #534ab7;
}
.ver-name {
  font-size: 13px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ver-fx {
  font-size: 10px;
  opacity: 0.8;
}
.ver-tag {
  flex-shrink: 0;
}
.ver-ops {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
.ver-expand {
  align-self: flex-start;
  margin-top: 4px;
  padding: 3px 10px;
  font-size: 12px;
  font-family: inherit;
  color: #534ab7;
  background: #f3f4ff;
  border: 1px dashed #dcd9f6;
  border-radius: 6px;
  cursor: pointer;
}
.ver-expand:hover {
  background: #e8e6ff;
}
@media (max-width: 760px) {
  .tool-card {
    flex-direction: column;
  }
  .id-panel {
    width: auto;
    border-right: none;
    border-bottom: 1px solid #eef0f3;
  }
}
</style>