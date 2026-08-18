<script setup lang="ts">
// 日志 —— 日志查看器 · 级别过滤 · 复制/导出 · 打开目录
import { computed, watch } from "vue";
import { NButton, NSelect, NCard } from "naive-ui";
import { useApp, store, showMsg } from "../store";
import EmptyState from "../components/EmptyState.vue";

const app = useApp();

// 按行分割（兼容 \r\n 与 \n），供渲染逐行着色
const logLines = computed(() => app.logsContent.split(/\r?\n/));

// 打开日志目录：用系统文件管理器（explorer / open）
function openDir() {
  const dir = app.logsDir || app.bootstrap?.logsDir;
  if (!dir) return showMsg("日志目录未知");
  const cmd = navigator.platform.toLowerCase().includes("win") ? "explorer" : "open";
  void store
    .run<void>("spawn_with_env", { command: cmd, args: [dir] })
    .then(() => showMsg("已打开日志目录"))
    .catch(() => showMsg("打开目录失败（预览模式下不可用）"));
}

function exportLogs() {
  if (!app.logsContent) return showMsg("暂无日志内容");
  void navigator.clipboard?.writeText(app.logsContent).then(
    () => showMsg("日志已复制到剪贴板"),
    () => showMsg("复制失败（预览模式下不可用）")
  );
}

// 切换文件 / 级别时自动重读
watch(
  [() => app.logFile, () => app.logLevel],
  () => {
    if (app.logFile || app.logsFiles.length > 0) {
      void store
        .run<string>("read_logs", { file: app.logFile || null, level: app.logLevel })
        .then((c) => (app.logsContent = c))
        .catch(() => {});
    }
  }
);

function levelColor(line: string): string {
  if (line.includes(" ERROR ") || line.includes(" ERROR]")) return "lv-error";
  if (line.includes(" WARN ") || line.includes(" WARN]")) return "lv-warn";
  if (line.includes(" INFO ") || line.includes(" INFO]")) return "lv-info";
  return "lv-muted";
}

const fileOptions = () => app.logsFiles.map((f) => ({ label: f.name, value: f.name }));
const levelOptions = [
  { label: "全部", value: "ALL" },
  { label: "INFO", value: "INFO" },
  { label: "WARN", value: "WARN" },
  { label: "ERROR", value: "ERROR" },
];
</script>

<template>
  <section class="section">
    <n-card size="small" title="日志查看器" class="section-card" :bordered="true">
      <template #header-extra>
        <n-button size="small" @click="openDir">打开目录</n-button>
        <n-button size="small" :disabled="!app.logsContent" @click="exportLogs">导出 / 复制</n-button>
        <n-button size="small" @click="store.refreshLogs()">刷新</n-button>
      </template>
      <!--工具条：文件切换 + 级别过滤 -->
      <div class="log-toolbar">
        <span class="proxy-label">文件：</span>
        <n-select
          :value="app.logFile || app.logsFiles[0]?.name || ''"
          :options="fileOptions()"
          :placeholder="app.logsFiles.length === 0 ? '（无日志文件）' : '选择文件'"
          size="small"
          style="width: 240px"
          @update:value="(v: string) => (app.logFile = v)"
        />
        <span class="proxy-label">级别：</span>
        <n-select
          :value="app.logLevel"
          :options="levelOptions"
          size="small"
          style="width: 110px"
          @update:value="(v: string) => (app.logLevel = v)"
        />
        <span class="mono muted">
          {{ app.logsFiles.length > 0 ? `${app.logsFiles.length} 个文件 · ${app.logsDir || "…"}` : "" }}
        </span>
      </div>

      <!-- 日志内容 -->
      <div v-if="app.logsContent" class="log-viewer">
        <div
          v-for="(line, i) in logLines"
          :key="i"
          class="log-line"
          :class="levelColor(line)"
        >{{ line }}</div>
      </div>
      <EmptyState
        v-else
        hint="应用启动后自动写入 ~/.envhive/logs/envhive.log"
      />
    </n-card>
  </section>
</template>

<style scoped>
.log-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.section-card {
  margin-bottom: 16px;
}
.section-card :deep(.n-card-header) {
  padding-bottom: 10px;
}
.proxy-label {
  font-size: 13px;
  font-weight: 600;
}
.log-viewer {
  background: #0f1115;
  color: #d4d4d8;
  border-radius: 10px;
  padding: 14px 16px;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
  font-size: 12px;
  line-height: 1.6;
  max-height: calc(100vh - 320px);
  overflow: auto;
}
.log-line {
  white-space: pre-wrap;
  word-break: break-all;
}
.lv-error {
  color: #f87171;
}
.lv-warn {
  color: #fbbf24;
}
.lv-info {
  color: #60a5fa;
}
.lv-muted {
  color: #71717a;
}
</style>
