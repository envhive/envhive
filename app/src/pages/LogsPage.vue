<script setup lang="ts">
// 日志 —— 日志查看器 · 级别过滤 · 复制/导出 · 打开目录
import { computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { NButton, NSelect, NCard } from "naive-ui";
import { useApp, store, showMsg } from "../store";
import EmptyState from "../components/EmptyState.vue";

const app = useApp();
const { t } = useI18n();

// 按行分割（兼容 \r\n 与 \n），供渲染逐行着色
const logLines = computed(() => app.logsContent.split(/\r?\n/));

// 打开日志目录：用系统文件管理器（explorer / open）
function openDir() {
  const dir = app.logsDir || app.bootstrap?.logsDir;
  if (!dir) return showMsg(t("logs.msgDirUnknown"));
  const cmd = navigator.platform.toLowerCase().includes("win") ? "explorer" : "open";
  void store
    .run<void>("spawn_with_env", { command: cmd, args: [dir] })
    .then(() => showMsg(t("logs.msgDirOpened")))
    .catch(() => showMsg(t("logs.msgDirOpenFailed")));
}

function exportLogs() {
  if (!app.logsContent) return showMsg(t("logs.msgNoContent"));
  void navigator.clipboard?.writeText(app.logsContent).then(
    () => showMsg(t("logs.msgCopied")),
    () => showMsg(t("logs.msgCopyFailed"))
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
const levelOptions = computed(() => [
  { label: t("logs.levelAll"), value: "ALL" },
  { label: "INFO", value: "INFO" },
  { label: "WARN", value: "WARN" },
  { label: "ERROR", value: "ERROR" },
]);
</script>

<template>
  <section class="section">
    <n-card size="small" :title="t('logs.title')" class="section-card" :bordered="true">
      <template #header-extra>
        <n-button size="small" @click="openDir">{{ t("common.openDir") }}</n-button>
        <n-button size="small" :disabled="!app.logsContent" @click="exportLogs">{{ t("logs.exportCopy") }}</n-button>
        <n-button size="small" @click="store.refreshLogs()">{{ t("common.refresh") }}</n-button>
      </template>
      <!--工具条：文件切换 + 级别过滤 -->
      <div class="log-toolbar">
        <span class="proxy-label">{{ t("logs.file") }}</span>
        <n-select
          :value="app.logFile || app.logsFiles[0]?.name || ''"
          :options="fileOptions()"
          :placeholder="app.logsFiles.length === 0 ? t('logs.noLogFiles') : t('logs.selectFile')"
          size="small"
          style="width: 240px"
          @update:value="(v: string) => (app.logFile = v)"
        />
        <span class="proxy-label">{{ t("logs.level") }}</span>
        <n-select
          :value="app.logLevel"
          :options="levelOptions"
          size="small"
          style="width: 110px"
          @update:value="(v: string) => (app.logLevel = v)"
        />
        <span class="mono muted">
          {{ app.logsFiles.length > 0 ? t("logs.fileCount", { count: app.logsFiles.length, dir: app.logsDir || "…" }) : "" }}
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
        :hint="t('logs.emptyHint')"
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
