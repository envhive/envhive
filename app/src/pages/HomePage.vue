<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { NButton, NCard, NCollapse, NCollapseItem, NModal, NForm, NFormItem, NInput, NAlert, NTooltip, NPopselect } from "naive-ui";
import { useApp, store, showMsg } from "../store";
import EnvTable from "../components/EnvTable.vue";
import ProjectEnvRow from "../components/ProjectEnvRow.vue";
import VersionComboPicker from "../components/VersionComboPicker.vue";
import StatusChip from "../components/StatusChip.vue";
import EmptyState from "../components/EmptyState.vue";
import SkeletonCard from "../components/SkeletonCard.vue";
import type { ProjectPreset, ProjectToolVersion, ToolEnvInfo } from "../types";
import ToolIcon from "../components/ToolIcon.vue";

const app = useApp();
const { t } = useI18n();

const editorOpen = ref(false);
const editing = ref<ProjectPreset | null>(null);
const presetName = ref("");
const presetDir = ref("");
const presetVersions = ref<ProjectToolVersion[]>([]);

const activeTools = computed(() => app.home?.tools.filter((s) => s.active) ?? []);
const envVarCount = computed(() => app.home?.merged.filter((r) => r.key !== "PATH").length ?? 0);
const pathCount = computed(() => app.home?.merged.filter((r) => r.key === "PATH").length ?? 0);

function openNew() {
  editing.value = null;
  presetName.value = "";
  presetDir.value = "";
  presetVersions.value = [];
  editorOpen.value = true;
}

function openEdit(p: ProjectPreset) {
  editing.value = p;
  presetName.value = p.name;
  presetDir.value = p.dir;
  presetVersions.value = p.versions.map((v) => ({ ...v }));
  editorOpen.value = true;
}

function cloneGlobal() {
  const list: ProjectToolVersion[] = [];
  for (const s of app.tools) {
    if (s.current) {
      list.push({
        tool: s.name,
        distribution: s.distributions?.length ? (s.defaultDistribution ?? s.distributions[0].key) : undefined,
        version: s.current,
      });
    }
  }
  presetVersions.value = list;
  showMsg(t("home.msg.clonedGlobal"));
}

function doSave() {
  if (!presetName.value.trim()) return showMsg(t("home.msg.nameRequired"));
  if (!presetDir.value.trim()) return showMsg(t("home.msg.dirRequired"));
  if (presetVersions.value.some((v) => !v.version)) return showMsg(t("home.msg.versionRequired"));
  void store.saveProjectPreset({
    name: presetName.value.trim(),
    dir: presetDir.value.trim(),
    versions: presetVersions.value,
  }).then(() => {
    editorOpen.value = false;
  });
}

/** 首页概览条目对应的完整工具信息（含 installed/current 等） */
function fullTool(s: ToolEnvInfo) {
  return app.tools.find((x) => x.name === s.name);
}

/** 该工具的已安装版本列表（与工具管理同一来源 app.tools.installed） */
function installedOf(s: ToolEnvInfo): string[] {
  return fullTool(s)?.installed ?? [];
}

/** 切换下拉的选项：已安装版本，当前版本追加「· 当前」标注 */
function switchOptions(s: ToolEnvInfo) {
  return installedOf(s).map((v) => ({
    label: v === s.version ? t("home.global.currentSuffix", { version: v }) : v,
    value: v,
  }));
}

/** 切换是否可用：已安装版本 > 1 才有切换意义 */
function switchDisabled(s: ToolEnvInfo): boolean {
  return installedOf(s).length <= 1;
}

/** 在下拉中选中版本即切换（n-popselect 选中后自动收起） */
function onSwitchSelect(s: ToolEnvInfo, v: string) {
  if (!v || v === s.version) return; // 选中的就是当前版本：无需切换
  const full = fullTool(s);
  if (!full) return;
  void store.switchVersion(full, v);
}

/** 解除某工具的全局环境变量（复用工具管理的「取消使用」逻辑） */
function unuseGlobalFor(s: ToolEnvInfo) {
  const full = fullTool(s);
  if (full) void store.unuseGlobal(full);
}

/**工具分类的友好显示：language → 语言；build → 构建工具；其他原样（如未来新增的 maven 等） */
function categoryLabel(c: string | null | undefined): string {
  const map: Record<string, string> = {
    language: t("home.category.language"),
    build: t("home.category.build"),
    tool: t("home.category.tool"),
  };
  return (c && map[c]) || c || "";
}
</script>

<template>
  <section class="section">
    <p class="page-desc">{{ t("home.desc") }}</p>

    <n-alert v-if="!app.home && !app.backend" type="warning" :bordered="false" class="preview-banner">
      {{ t("common.previewBanner") }}
    </n-alert>

    <!-- ============ 全局环境（系统级） ============ -->
    <n-card size="small" :title="t('home.global.title')" class="section-card" :bordered="true">
      <template #header-extra>
        <span class="home-summary mono">
          {{ t("home.global.summary", { tools: activeTools.length, vars: envVarCount, paths: pathCount }) }}
        </span>
      </template>

      <template v-if="!app.home">
        <div class="grid">
          <SkeletonCard /><SkeletonCard /><SkeletonCard />
        </div>
      </template>

      <EmptyState
        v-else-if="activeTools.length === 0"
        :hint="t('home.global.emptyHint')"
        :action-label="t('home.global.emptyAction')"
        @action="app.page = 'tools'"
      />

      <div v-else class="grid tool-mini-grid">
        <div v-for="(s, i) in activeTools" :key="s.name" class="card tool-mini-card">
          <!-- 头部：左侧 = 圆点 + 名称；右侧 = 分类徽章（语言 / 构建工具） -->
          <div class="mini-head">
            <div class="mini-head-left">
              <ToolIcon :icon="s.icon" :index="i" :size="20" :name="s.display" />
              <span class="mini-name">{{ s.display }}</span>
            </div>
            <StatusChip v-if="s.category" tone="info">{{ categoryLabel(s.category) }}</StatusChip>
          </div>

          <!-- 大版本号（mono） -->
          <div class="mini-version">{{ s.version ?? t("home.global.noVersion") }}</div>

          <!-- 元信息：已装数量 -->
          <div class="mini-meta">
            <span>{{ t("home.global.installedCount", { count: installedOf(s).length }) }}</span>
          </div>

          <div class="mini-foot">
            <!-- 解除：轻量按钮（图标 + 文本） -->
            <n-tooltip trigger="hover" placement="top-end">
              <template #trigger>
                <n-button
                  size="small"
                  quaternary
                  class="act-btn"
                  :disabled="app.busy === s.name"
                  @click="unuseGlobalFor(s)"
                >
                  <template #icon>
                    <svg
                      width="14"
                      height="14"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    >
                      <rect width="18" height="11" x="3" y="11" rx="2" ry="2" />
                      <path d="M7 11V7a5 5 0 0 1 9.9-1" />
                    </svg>
                  </template>
                  {{ t("home.global.unuse") }}
                </n-button>
              </template>
              {{ t("home.global.unuseTip") }}
            </n-tooltip>
            <!-- 切换：主按钮（图标 + 文本） -->
            <n-popselect
              class="switch-pop"
              :value="s.version ?? ''"
              :options="switchOptions(s)"
              :disabled="switchDisabled(s) || app.busy === s.name"
              trigger="click"
              placement="bottom-end"
              size="small"
              scrollable
              :title="t('home.global.switchTip', { count: installedOf(s).length })"
              @update:value="(v: string) => onSwitchSelect(s, v)"
            >
              <n-button
                size="small"
                type="primary"
                class="act-btn"
                :disabled="switchDisabled(s) || app.busy === s.name"
                :loading="app.busy === s.name"
              >
                <template #icon>
                  <svg
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                  >
                    <path d="M8 3 4 7l4 4" />
                    <path d="M4 7h16" />
                    <path d="m16 21 4-4-4-4" />
                    <path d="M20 17H4" />
                  </svg>
                </template>
                {{ switchDisabled(s) ? t("home.global.onlyVersion") : t("home.global.switch") }}
              </n-button>
            </n-popselect>
          </div>
        </div>
      </div>

      <!-- 环境变量总览（默认折叠） -->
      <n-collapse v-if="app.home && app.home.merged.length > 0" class="env-collapse">
        <n-collapse-item name="env" title="">
          <template #header>
            <span>{{ t("home.global.envOverview", { count: app.home.merged.length }) }}</span>
          </template>
          <EnvTable :rows="app.home.merged" />
        </n-collapse-item>
      </n-collapse>
    </n-card>

    <!-- ============ 项目环境（会话级） ============ -->
    <n-card size="small" :title="t('home.project.title')" class="section-card" :bordered="true">
      <template #header-extra>
        <n-button size="small" quaternary @click="openNew">{{ t("home.project.newPreset") }}</n-button>
      </template>
      <p class="muted proj-hint">
        {{ t("home.project.hint") }}
      </p>

      <EmptyState
        v-if="app.projects.length === 0"
        :hint="t('home.project.emptyHint')"
        :action-label="t('home.project.emptyAction')"
        @action="openNew"
      />
      <div v-else class="proj-list">
        <ProjectEnvRow
          v-for="p in app.projects"
          :key="p.name"
          :preset="p"
          @edit="openEdit"
        />
      </div>
    </n-card>

    <!-- ============ 新建/编辑预设弹层 ============ -->
    <n-modal
      v-model:show="editorOpen"
      preset="card"
      :title="editing ? t('home.project.editTitle', { name: editing.name }) : t('home.project.newTitle')"
      style="width: 640px"
      :mask-closable="true"
    >
      <n-form label-placement="left" label-width="60" style="margin-bottom: 12px">
        <n-form-item :label="t('home.project.nameLabel')">
          <n-input v-model:value="presetName" :placeholder="t('home.project.namePlaceholder')" />
        </n-form-item>
        <n-form-item :label="t('home.project.dirLabel')">
          <n-input v-model:value="presetDir" :placeholder="t('home.project.dirPlaceholder')" />
        </n-form-item>
      </n-form>

      <VersionComboPicker
        :versions="presetVersions"
        @change="(v: ProjectToolVersion[]) => (presetVersions = v)"
        @clone-global="cloneGlobal"
      />

      <template #footer>
        <div class="modal-foot">
          <n-button @click="editorOpen = false">{{ t("common.cancel") }}</n-button>
          <n-button type="primary" @click="doSave">{{ t("home.project.save") }}</n-button>
        </div>
      </template>
    </n-modal>
  </section>
</template>

<style scoped>
.page-desc {
  font-size: 12.5px;
  color: #6b7280;
  margin-bottom: 14px;
}
.preview-banner {
  margin-bottom: 14px;
}
.section-card {
  margin-bottom: 16px;
}
.section-card :deep(.n-card-header) {
  padding-bottom: 10px;
}
.home-summary {
  font-size: 12px;
  color: #6b7280;
  display: flex;
  align-items: center;
  gap: 8px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 14px;
}
/* 全局环境卡片：分层布局 */
.tool-mini-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  border: 1px solid #e5e7eb;
  background: #fff;
  border-radius: 12px;
  padding: 16px 18px;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}
.tool-mini-card:hover {
  border-color: #d8dce3;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.04);
}
.mini-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.mini-head-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.mini-name {
  font-weight: 600;
  font-size: 14px;
  color: #111827;
}
.mini-version {
  font-family: "SFMono-Regular", Consolas, monospace;
  font-size: 22px;
  font-weight: 700;
  color: #1f2937;
  letter-spacing: -0.01em;
  line-height: 1.1;
}
.mini-meta {
  font-size: 12px;
  color: #6b7280;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.mini-foot {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  margin-top: auto;
  padding-top: 10px;
  border-top: 1px dashed #eef0f3;
}
/* 底部操作按钮：统一尺寸（图标 + 文本） */
.act-btn {
  min-width: 88px;
}
.switch-pop {
  min-width: 76px;
}
/* 兼容旧的 .card 通用样式（项目区域仍可能用到） */
.card {
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  padding: 14px 16px;
}
.env-collapse {
  margin-top: 12px;
}
.proj-hint {
  font-size: 12px;
  margin-bottom: 10px;
}
.proj-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.modal-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
