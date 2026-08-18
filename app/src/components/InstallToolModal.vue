<script setup lang="ts">
// InstallToolModal —— 安装新版本弹窗：发行商 × 版本列表单选 + 拉取 + 安装 + 实时进度
// 低频操作从卡片中剥离，弹窗内提供充足空间展示版本列表与下载进度。
import { computed, watch } from "vue";
import { NModal, NSelect, NButton, NProgress, NTag } from "naive-ui";
import { useApp, store } from "../store";
import ToolIcon from "./ToolIcon.vue";
import type { ToolInfo } from "../types";
import { compareVersions, versionsKey, lastNonFx, fmtSpeed, fmtBytes } from "../types";

const props = defineProps<{ show: boolean; tool: ToolInfo | null }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void }>();

const app = useApp();

const sel = computed(() => (props.tool ? app.selections[props.tool.name] ?? { version: "" } : { version: "" }));
const key = computed(() => (props.tool ? versionsKey(props.tool.name, sel.value.dist) : ""));
const hasDist = computed(() => !!(props.tool?.distributions && props.tool.distributions.length > 0));
const refreshing = computed(() => !!props.tool && !!app.refreshing[key.value]);
const busyTool = computed(() => !!props.tool && app.busy === props.tool.name);
const isInstalling = computed(() => busyTool.value && !!props.tool && app.progress?.tool === props.tool.name);
const selVersionInstalled = computed(() => !!sel.value.version && !!props.tool?.installed?.includes(sel.value.version));

const distOptions = computed(() => (props.tool?.distributions ?? []).map((d) => ({ label: d.display, value: d.key })));

// 版本列表：发行商缓存 → 插件 available 兜底 → 空
const cachedVersions = computed(() => {
  if (!props.tool) return [];
  return app.versionsMap[key.value] ?? props.tool.available ?? [];
});

const versionList = computed(() => cachedVersions.value.slice().sort(compareVersions).reverse());

// 加速镜像：插件 TOOL.mirrors 声明时展示；选「官方源（不镜像）」即恢复官方源，无需总开关
const currentMirror = computed(() => (props.tool ? store.currentToolMirror(props.tool) : null));
// 同 name 可能对应多条规则（如 maven 同时覆盖 dlcdn 与 archive），下拉按 name 去重
const mirrorOptions = computed(() => {
  if (!props.tool?.mirrors || props.tool.mirrors.length === 0) return [];
  const seen = new Set<string>();
  const opts: { label: string; value: string }[] = [{ label: "官方源", value: "" }];
  for (const m of props.tool.mirrors) {
    if (m.from === m.to) continue; // 官方占位不进下拉
    if (seen.has(m.name)) continue;
    seen.add(m.name);
    opts.push({ label: m.name, value: m.name });
  }
  return opts;
});
// 当前生效镜像规则的多行展示：同名多 from 时合并，避免"看不清覆盖范围"
const mirrorRuleText = computed(() => {
  if (!props.tool?.mirrors?.length) return "";
  const cur = store.currentToolMirror(props.tool);
  if (cur) {
    const lines = props.tool.mirrors.filter((x) => x.name === cur && x.from !== x.to);
    if (lines.length === 0) return "";
    return lines.map((m) => `${trimProto(m.from)} → ${trimProto(m.to)}`).join("\n");
  }
  // 官方源：展示所有官方 from（去重）
  const officials = Array.from(
    new Set(props.tool.mirrors.filter((x) => x.from !== x.to).map((x) => x.from))
  );
  return officials.map(trimProto).join("\n");
});
function trimProto(u: string): string {
  return u.replace(/^https?:\/\//, "").replace(/\/$/, "");
}
function onMirrorChange(v: string) {
  if (!props.tool) return;
  void store.setToolMirror(props.tool, v || null);
}

const STAGE_TEXT: Record<string, string> = {
  resolving: "解析版本",
  downloading: "下载中",
  verifying: "校验中",
  extracting: "解压中",
  done: "完成",
  failed: "失败",
};

function stageText(stage: string): string {
  return STAGE_TEXT[stage] ?? "执行中";
}

const installBtnText = computed(() => {
  if (isInstalling.value) return "安装中…";
  if (!sel.value.version) return "请选择版本";
  if (selVersionInstalled.value) return "该版本已安装";
  return `安装 ${sel.value.version}`;
});

// 打开弹窗：该发行商下无缓存时自动拉取一次版本列表
watch(
  () => props.show,
  (v) => {
    if (v && props.tool) {
      const k = versionsKey(props.tool.name, sel.value.dist);
      if (!app.versionsMap[k] && !props.tool.available) void store.loadVersions(props.tool, false);
    }
  }
);

// 切换发行商：新发行商无缓存时自动拉取
watch(
  () => sel.value.dist,
  (d, old) => {
    if (props.tool && d && d !== old && !app.versionsMap[versionsKey(props.tool.name, d)]) {
      void store.loadVersions(props.tool, false);
    }
  }
);

// 安装完成（入队后 worker 完成下载）→ 关闭弹窗，卡片自动刷新
watch(
  () => app.progress?.stage,
  (s) => {
    if (s === "done" && props.tool) emit("update:show", false);
  }
);

// 列表回填：切发行商清空版本后，若版本列表已就绪则回填一个非 FX 版本
watch(versionList, (list) => {
  if (!props.tool || !sel.value.version || selVersionInstalled.value) return;
  if (!list.includes(sel.value.version)) {
    const v = lastNonFx(list) || list[0];
    if (v) app.selections[props.tool.name] = { ...sel.value, version: v };
  }
});
</script>

<template>
  <n-modal
    :show="show"
    :on-update:show="(v: boolean) => emit('update:show', v)"
    preset="card"
    style="width: 720px; max-width: 92vw"
    :mask-closable="!busyTool"
    :close-on-esc="!busyTool"
  >
    <template v-if="tool" #header>
      <span class="modal-title">
        <ToolIcon :icon="tool.icon" :size="18" :name="tool.display" />
        <span>安装 {{ tool.display }} 新版本</span>
      </span>
    </template>
    <div v-if="tool" class="install-modal">
      <!-- 发行商 × 拉取 -->
      <div class="ctrl-row">
        <span v-if="hasDist" class="lbl">发行商</span>
        <n-select
          v-if="hasDist"
          size="small"
          class="ctrl-select"
          :value="sel.dist ?? ''"
          :options="distOptions"
          :disabled="busyTool"
          placeholder="发行商"
          @update:value="(d: string) => (app.selections[tool!.name] = { dist: d, version: '' })"
        />
        <n-button
          size="small"
          :loading="refreshing"
          :disabled="busyTool || refreshing"
          @click="store.loadVersions(tool, true)"
        >
          拉取
        </n-button>
        <span class="muted list-count">
          {{ cachedVersions.length ? `${cachedVersions.length} 个版本` : "暂无缓存，点击「拉取」获取版本列表" }}
        </span>
      </div>

      <!-- 加速镜像（可选）：插件 TOOL.mirrors 声明时显示；选官方源即不镜像 -->
      <div v-if="mirrorOptions.length > 0" class="ctrl-row">
        <span class="lbl">加速镜像</span>
        <n-select
          size="small"
          class="ctrl-select"
          :value="currentMirror ?? ''"
          :options="mirrorOptions"
          :disabled="busyTool"
          placeholder="选择镜像…"
          @update:value="onMirrorChange"
        />
        <span v-if="mirrorRuleText" class="muted mirror-rule mono" :title="mirrorRuleText">{{ mirrorRuleText }}</span>
      </div>

      <!-- 版本列表：单选 -->
      <div v-if="versionList.length > 0" class="ver-list">
        <label
          v-for="v in versionList"
          :key="v"
          class="ver-item"
          :class="{ cur: tool.current === v, inst: tool.installed.includes(v) && tool.current !== v }"
        >
          <input
            type="radio"
            class="ver-radio"
            :checked="sel.version === v"
            :disabled="busyTool"
            @change="app.selections[tool.name] = { ...sel, version: v }"
          />
          <span class="mono ver-code">
            {{ v }}<span v-if="v.includes('.fx-')" class="ver-fx"> FX</span>
          </span>
          <span class="ver-status">
            <n-tag v-if="tool.current === v" size="tiny" type="primary" :bordered="false" round>当前</n-tag>
            <n-tag v-else-if="tool.installed.includes(v)" size="tiny" type="success" :bordered="false" round>
              已安装
            </n-tag>
          </span>
        </label>
      </div>
      <div v-else class="empty-box muted">没有可用版本，点击「拉取」从版本源获取</div>

      <!-- 下载进度 -->
      <div v-if="isInstalling && app.progress" class="progress-box">
        <n-progress
          type="line"
          :percentage="Math.max(0, Math.min(100, app.progress.percent))"
          :height="6"
          :show-indicator="false"
          status="success"
        />
        <div class="progress-meta mono">
          {{ stageText(app.progress.stage) }} · {{ app.progress.percent.toFixed(0) }}%
          <template v-if="app.progress.speedMbps != null"> · {{ fmtSpeed(app.progress.speedMbps) }}</template>
          <template v-if="app.progress.totalBytes != null">
            · {{ fmtBytes(app.progress.downloadedBytes ?? 0) }} / {{ fmtBytes(app.progress.totalBytes) }}
          </template>
          <template v-else-if="app.progress.downloadedBytes != null">
            · {{ fmtBytes(app.progress.downloadedBytes) }}
          </template>
        </div>
      </div>

      <!-- 底部 -->
      <div class="foot">
        <n-button size="small" :disabled="busyTool" @click="emit('update:show', false)">关闭</n-button>
        <n-button
          size="small"
          type="primary"
          :loading="isInstalling"
          :disabled="!sel.version || busyTool || selVersionInstalled"
          @click="store.install(tool)"
        >
          {{ installBtnText }}
        </n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
.modal-title {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.install-modal {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ctrl-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.lbl {
  font-size: 11px;
  color: #9ca3af;
  flex-shrink: 0;
}
.ctrl-select {
  width: 220px;
}
.list-count {
  font-size: 11px;
}
.mirror-rule {
  font-size: 11px;
  white-space: pre-line;
  word-break: break-all;
  line-height: 1.4;
  display: block;
  margin-top: 2px;
}
.ver-list {
  max-height: 260px;
  overflow-y: auto;
  border: 1px solid #eef0f3;
  border-radius: 10px;
  display: flex;
  flex-direction: column;
}
.ver-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 12px;
  cursor: pointer;
  border-bottom: 1px solid #f3f4f6;
}
.ver-item:last-child {
  border-bottom: none;
}
.ver-item:hover {
  background: #fafbfc;
}
.ver-item.cur {
  background: #f3f4ff;
}
.ver-item.inst {
  opacity: 0.75;
}
.ver-radio {
  accent-color: #534ab7;
  flex-shrink: 0;
}
.ver-code {
  font-size: 13px;
  white-space: nowrap;
}
.ver-fx {
  font-size: 10px;
  opacity: 0.8;
}
.ver-status {
  margin-left: auto;
  display: flex;
  gap: 6px;
}
.empty-box {
  border: 1px dashed #e5e7eb;
  border-radius: 10px;
  padding: 18px;
  text-align: center;
  font-size: 12px;
}
.progress-box {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  background: #fafbfc;
  border-radius: 10px;
}
.progress-meta {
  font-size: 11px;
  color: #6b7280;
}
.foot {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 2px;
}
</style>
