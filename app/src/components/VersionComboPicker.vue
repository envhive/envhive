<script setup lang="ts">
// VersionComboPicker —— 版本组合选择器（新建/编辑项目预设）
// 每工具一行：名称 + 版本下拉（多发行商再加发行商下拉）+ 移除；「从全局克隆」一键带入当前全局版本
// 版本下拉仅列出「已下载安装到本地」的版本（tool.installed），远程可下载版本不在此处展示。
import { computed } from "vue";
import { NButton, NSelect, NDropdown, type SelectOption } from "naive-ui";
import { useApp } from "../store";
import ToolIcon from "./ToolIcon.vue";
import type { ProjectToolVersion, ToolInfo } from "../types";
import { lastNonFx } from "../types";

const props = defineProps<{
  versions: ProjectToolVersion[];
}>();

const emit = defineEmits<{
  change: [v: ProjectToolVersion[]];
  "clone-global": [];
}>();

const app = useApp();

function update(i: number, patch: Partial<ProjectToolVersion>) {
  emit("change", props.versions.map((v, idx) => (idx === i ? { ...v, ...patch } : v)));
}

/** 全部可选的工具（行首切换用，允许重复选择） */
function allToolOptions() {
  return app.tools.map((s) => ({ label: s.display, value: s.name }));
}

/** 「+ 添加工具」候选：尚未加入当前组合的工具（dropdown 用 key 标识） */
const addOptions = computed(() =>
  app.tools
    .filter((s) => !props.versions.some((v) => v.tool === s.name))
    .map((s) => ({ label: s.display, key: s.name }))
);

/** 添加指定工具一行（默认选中默认发行商 + 已安装版本） */
function addTool(sdkName: string) {
  const s = app.tools.find((x) => x.name === sdkName);
  if (!s) return;
  emit("change", [
    ...props.versions,
    {
      tool: s.name,
      distribution: s.distributions?.length ? (s.defaultDistribution ?? s.distributions[0].key) : undefined,
      version: defaultVersion(s),
    },
  ]);
}

/** 行内切换工具：重置发行商 / 版本 */
function onToolChange(i: number, sdkName: string) {
  const s = app.tools.find((x) => x.name === sdkName);
  if (!s) return;
  update(i, {
    tool: sdkName,
    distribution: s.distributions?.length ? (s.defaultDistribution ?? s.distributions[0].key) : undefined,
    version: defaultVersion(s),
  });
}

function removeRow(i: number) {
  emit("change", props.versions.filter((_, idx) => idx !== i));
}

/** 该行工具的图标 data URI（无图标为 undefined，前端回退圆点） */
function sdkIcon(sdkName: string): string | null | undefined {
  return app.tools.find((s) => s.name === sdkName)?.icon;
}

function distOptions(sdkName: string) {
  const tool = app.tools.find((s) => s.name === sdkName);
  return (tool?.distributions ?? []).map((d) => ({ label: d.display, value: d.key }));
}

/** 该工具的已安装版本列表（与工具管理同一来源 app.tools.installed） */
function installedOf(sdkName: string): string[] {
  return app.tools.find((s) => s.name === sdkName)?.installed ?? [];
}

/** 默认选中版本：全局当前版本（须已安装）→ 已安装最新版 → 空 */
function defaultVersion(s: ToolInfo): string {
  if (s.current && (s.installed ?? []).includes(s.current)) return s.current;
  return lastNonFx(s.installed ?? []);
}

/** 版本下拉：仅本地已安装版本；编辑旧预设时若当前值已卸载，追加不可选占位避免空白 */
function versionOptions(sdkName: string, current?: string) {
  const tool = app.tools.find((s) => s.name === sdkName);
  if (!tool) return [];
  const opts: SelectOption[] = (tool.installed ?? []).map((v) => ({ label: v, value: v }));
  if (current && !opts.some((o) => o.value === current)) {
    opts.unshift({ label: `${current}（未安装）`, value: current, disabled: true });
  }
  return opts;
}

function hasDist(sdkName: string) {
  const tool = app.tools.find((s) => s.name === sdkName);
  return !!(tool?.distributions && tool.distributions.length > 0);
}

const unknownRows = computed(() => props.versions.filter((v) => !app.tools.some((s) => s.name === v.tool)));

/** 候选工具的悬停提示（dropdown 不支持 slot label，用 native title 属性附在按钮上） */
function addTitle(): string {
  return addOptions.value.length === 0 ? "所有工具已加入组合" : "选择要添加的工具";
}
</script>

<template>
  <div class="vcp">
    <div class="vcp-head">
      <span class="proxy-label">版本组合</span>
      <div style="display: flex; gap: 6px">
        <n-button size="small" quaternary @click="emit('clone-global')">从全局克隆</n-button>
        <n-dropdown
          class="vcp-add"
          trigger="click"
          placement="bottom-end"
          :options="addOptions"
          :disabled="addOptions.length === 0"
          @select="(key: string | number) => addTool(String(key))"
        >
          <n-button size="small" quaternary :title="addTitle()">+ 添加工具</n-button>
        </n-dropdown>
      </div>
    </div>

    <p v-if="props.versions.length === 0" class="muted vcp-empty">
      尚未选择任何工具—— 点击「从全局克隆」或「+ 添加工具」开始。
    </p>

    <div v-else class="vcp-rows">
      <!-- 未知工具行 -->
      <div v-for="(item, i) in unknownRows" :key="`u-${i}`" class="vcp-row">
        <span class="mono muted">{{ item.tool }}</span>
        <span class="mono">@{{ item.version }}</span>
        <n-button size="tiny" quaternary @click="removeRow(i)">移除</n-button>
      </div>

      <!-- 已知工具行 -->
      <div
        v-for="(item, i) in versions.filter((v) => app.tools.some((s) => s.name === v.tool))"
        :key="`k-${i}`"
        class="vcp-row"
      >
        <ToolIcon :icon="sdkIcon(item.tool)" :index="i" :size="16" :name="item.tool" />
        <n-select
          size="small"
          class="vcp-tool"
          :value="item.tool"
          :options="allToolOptions()"
          placeholder="工具"
          @update:value="(v: string) => onToolChange(i, v)"
        />

        <n-select
          v-if="hasDist(item.tool)"
          size="small"
          class="vcp-select"
          :value="item.distribution ?? ''"
          :options="distOptions(item.tool)"
          placeholder="发行商"
          @update:value="(d: string) => update(i, { distribution: d, version: '' })"
        />

        <n-select
          size="small"
          class="vcp-select"
          :value="item.version"
          :options="versionOptions(item.tool, item.version)"
          :disabled="installedOf(item.tool).length === 0"
          :placeholder="installedOf(item.tool).length === 0 ? '无已安装版本' : '选择版本'"
          filterable
          @update:value="(v: string) => update(i, { version: v })"
        />

        <n-button size="tiny" quaternary @click="removeRow(i)">移除</n-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.vcp-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}
.vcp-empty {
  font-size: 12px;
}
.vcp-rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.vcp-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.vcp-tool {
  width: 130px;
  flex-shrink: 0;
}
.vcp-add {
  min-width: 76px;
}
.vcp-select {
  flex: 1;
  min-width: 0;
}
</style>
