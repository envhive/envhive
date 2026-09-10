<script setup lang="ts">
// QueueDrawer —— 全局悬浮队列抽屉（右下角 FAB + 抽屉）
// v2 优化：耗时展示 / 失败+取消可重试 / 全部取消+清空已完成 / ETA / 阶段脉动条 / URL 折叠复制 / FAB 空队列隐藏
import { computed, reactive } from "vue";
import { useI18n } from "vue-i18n";
import {
  NButton,
  NDrawer,
  NDrawerContent,
  NBadge,
  NProgress,
  NTag,
  NFloatButton,
} from "naive-ui";
import { useApp, store, showErr, showMsg, errMsg } from "../store";
import { stageLabel, taskStatusLabel } from "../i18n";
import {
  fmtSpeed,
  fmtBytes,
  fmtDuration,
  fmtDurationSec,
  fmtDateTime,
} from "../types";
import type { QueueTask, QueueEnqueueResult } from "../types";

const app = useApp();
const { t } = useI18n();

const runningCount = computed(
  () => app.queue.filter((task) => task.status === "running" || task.status === "queued").length
);

const hasFinished = computed(() =>
  app.queue.some((task) => task.status === "done" || task.status === "failed" || task.status === "cancelled")
);

const statusType: Record<QueueTask["status"], "info" | "success" | "error" | "warning" | "default"> = {
  queued: "info",
  running: "warning",
  done: "success",
  failed: "error",
  cancelled: "default",
};

// 运行中任务显示实时阶段文本（合并 download-progress 回填的 stage），其余显示任务状态
function statusLabel(task: QueueTask): string {
  if (task.status === "running" && task.stage) return stageLabel(task.stage);
  return taskStatusLabel(task.status);
}

// 进度摘要：下载中显示 百分比 + 速度（<1MB/s 自动转 KB/s）；其余阶段只显示百分比
function progressText(task: QueueTask): string {
  const pct = `${Math.max(0, Math.min(100, task.percent)).toFixed(0)}%`;
  if (task.stage === "downloading" && task.speedMbps != null) {
    return `${pct} · ${fmtSpeed(task.speedMbps)}`;
  }
  return pct;
}

// ETA：下载中且大小/速度齐全时，按 (剩余字节 / 速度) 估算剩余时间
function etaText(task: QueueTask): string {
  if (task.stage !== "downloading" || task.speedMbps == null || task.speedMbps <= 0) return "";
  if (task.totalBytes == null || task.downloadedBytes == null) return "";
  const remain = (task.totalBytes - task.downloadedBytes) / (task.speedMbps * 1024 * 1024);
  if (remain <= 0) return "";
  return t("queue.remaining", { time: fmtDurationSec(remain) });
}

// 终态任务耗时（createdAt → finishedAt）；非终态返回空串
function durationText(task: QueueTask): string {
  return fmtDuration(task.createdAt, task.finishedAt);
}

// ---- 取消：乐观禁用，防止重复点击 ----
const cancellingIds = reactive(new Set<number>());
async function cancelTask(task: QueueTask) {
  if (cancellingIds.has(task.id)) return;
  cancellingIds.add(task.id);
  try {
    await store.cancelTask(task.id);
  } catch {
    /* store 内已提示 */
  } finally {
    cancellingIds.delete(task.id);
  }
}

// ---- 重试：失败 / 已取消 均可重新入队（后端按 tool+version+distribution 去重） ----
async function retry(task: QueueTask) {
  try {
    const r = await store.run<QueueEnqueueResult>("enqueue_install", {
      name: task.tool,
      version: task.version,
      distribution: task.distribution ?? null,
    });
    if (r.reused) {
      showMsg(t("toast.alreadyQueued", { tool: task.tool, version: task.version, id: r.id }));
    } else {
      showMsg(t("toast.requeued", { tool: task.tool, version: task.version, id: r.id }));
    }
  } catch (e) {
    showErr(t("toast.retryFailed", { msg: errMsg(e) }));
  }
}

// ---- 下载地址：折叠（单行省略）/ 展开 + 复制 ----
const expandedUrls = reactive(new Set<number>());
function toggleUrl(task: QueueTask) {
  if (expandedUrls.has(task.id)) expandedUrls.delete(task.id);
  else expandedUrls.add(task.id);
}
async function copyUrl(url: string) {
  try {
    await navigator.clipboard.writeText(url);
    showMsg(t("queue.urlCopied"));
  } catch {
    showErr(t("queue.urlCopyFailed"));
  }
}

function cancelAll() {
  void store.cancelAll();
}
function clearFinished() {
  void store.clearFinished();
}
</script>

<template>
  <!-- 悬浮按钮（带角标）：n-float-button 提供 fixed 定位，n-badge 用 offset 控制角标位置 -->
  <!-- 队列为空时不显示 FAB，避免常驻右下角遮挡内容 -->
  <n-float-button
    v-if="app.queue.length > 0"
    class="queue-fab"
    position="fixed"
    :right="28"
    :bottom="28"
    shape="circle"
    type="primary"
    @click="app.queueDrawerOpen = !app.queueDrawerOpen"
  >
    <n-badge :value="runningCount" :show="runningCount > 0" :max="99" :offset="[6, -8]">
      <span style="font-size: 15px; line-height: 1">⬇</span>
    </n-badge>
  </n-float-button>

  <n-drawer v-model:show="app.queueDrawerOpen" :width="460" placement="right">
    <n-drawer-content closable>
      <template #header>
        <div class="queue-header">
          <span class="queue-title">{{ t("queue.title") }}</span>
          <span class="queue-count muted">
            {{
              app.queue.length > 0
                ? t("queue.summary", { total: app.queue.length, running: runningCount })
                : t("queue.empty")
            }}
          </span>
          <div style="flex: 1" />
          <n-button v-if="runningCount > 0" size="tiny" quaternary type="error" @click="cancelAll">
            {{ t("queue.cancelAll") }}
          </n-button>
          <n-button v-if="hasFinished" size="tiny" quaternary @click="clearFinished">
            {{ t("queue.clearFinished") }}
          </n-button>
        </div>
      </template>

      <div v-if="app.queue.length === 0" class="muted queue-empty">
        {{ t("queue.emptyHint") }}
      </div>

      <div v-else class="queue-list">
        <div v-for="task in app.queue" :key="task.id" class="queue-item" :class="`is-${task.status}`">
          <div class="queue-item-head">
            <n-tag :type="statusType[task.status]" size="small" round>
              {{ statusLabel(task) }}
            </n-tag>
            <span class="queue-tool">
              {{ task.tool }} {{ task.version }}
              <span v-if="task.distribution" class="mono muted">· {{ task.distribution }}</span>
            </span>
            <!-- 终态耗时（createdAt → finishedAt） -->
            <span
              v-if="durationText(task)"
              class="queue-duration mono muted"
              :title="t('queue.enqueuedAt', { time: fmtDateTime(task.createdAt) })"
            >
              ⏱ {{ durationText(task) }}
            </span>
            <div style="flex: 1" />
            <n-button
              v-if="task.status === 'queued' || task.status === 'running'"
              size="tiny"
              quaternary
              type="error"
              :loading="cancellingIds.has(task.id)"
              :disabled="cancellingIds.has(task.id)"
              @click="cancelTask(task)"
            >
              {{ task.status === "running" ? t("queue.cancelDownload") : t("queue.cancel") }}
            </n-button>
            <n-button
              v-if="task.status === 'failed' || task.status === 'cancelled'"
              size="tiny"
              quaternary
              @click="retry(task)"
            >
              {{ t("queue.retry") }}
            </n-button>
          </div>

          <template v-if="task.status === 'running'">
            <!-- 下载中：真实进度条 + 百分比/速度；其余阶段（解析/校验/解压）：脉动条（indeterminate） -->
            <div v-if="task.stage === 'downloading'" class="queue-progress-head">
              <n-progress
                type="line"
                :percentage="Math.max(0, Math.min(100, task.percent))"
                :height="6"
                :show-indicator="false"
                status="success"
              />
              <span class="queue-progress-text mono">{{ progressText(task) }}</span>
            </div>
            <div v-else class="queue-progress-head">
              <div class="queue-indeterminate" role="progressbar" :aria-label="t('queue.runningAria')">
                <div class="queue-indeterminate-bar" />
              </div>
              <span class="queue-progress-text mono">
                {{ task.stage ? stageLabel(task.stage) : t("common.executingEllipsis") }}
              </span>
            </div>
            <!-- 下载中：已下载 / 总大小 + ETA（总大小未知时只显示已下载） -->
            <div v-if="task.stage === 'downloading'" class="queue-size mono">
              <template v-if="task.totalBytes != null">
                {{ fmtBytes(task.downloadedBytes ?? 0) }} / {{ fmtBytes(task.totalBytes) }}
              </template>
              <template v-else>
                {{ t("queue.downloaded", { size: fmtBytes(task.downloadedBytes ?? 0) }) }}
              </template>
              <span v-if="etaText(task)" class="queue-eta">· {{ etaText(task) }}</span>
            </div>
            <!-- 下载中：实际下载地址（默认单行省略，可展开；支持一键复制） -->
            <div v-if="task.stage === 'downloading' && task.url" class="queue-url-row">
              <a
                class="queue-download-url mono"
                :class="{ expanded: expandedUrls.has(task.id) }"
                :href="task.url"
                target="_blank"
                rel="noreferrer"
                :title="task.url"
              >
                <span class="dl-icon">🔗</span>
                <span class="queue-url-text">{{ task.url }}</span>
              </a>
              <n-button size="tiny" quaternary class="queue-url-btn" @click="toggleUrl(task)">
                {{ expandedUrls.has(task.id) ? t("common.collapse") : t("common.expand") }}
              </n-button>
              <n-button size="tiny" quaternary class="queue-url-btn" @click="copyUrl(task.url)">
                {{ t("common.copy") }}
              </n-button>
            </div>
          </template>
          <div
            v-if="task.message"
            class="queue-msg mono"
            :class="{ 'is-error': task.status === 'failed' }"
          >
            {{ task.message }}
          </div>
        </div>
      </div>
    </n-drawer-content>
  </n-drawer>
</template>

<style scoped>
.queue-fab {
  box-shadow: 0 6px 18px rgba(83, 74, 183, 0.35);
}
.queue-fab :deep(.n-badge-sup) {
  border: 2px solid #fff;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.25);
  font-weight: 600;
}
/* 抽屉头部：显式字重 700（微软雅黑原生 Bold，避免 500 合成加粗导致中文发虚） */
.queue-header {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
}
.queue-title {
  font-size: 16px;
  font-weight: 700;
  line-height: 1.4;
  letter-spacing: 0.2px;
}
.queue-count {
  font-size: 12px;
  color: #6b7280;
  white-space: nowrap;
}
.queue-empty {
  padding: 12px 4px;
}
.queue-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.queue-item {
  border: 1px solid #eef0f3;
  border-left-width: 3px;
  border-radius: 10px;
  padding: 10px 12px;
  background: #fafbfc;
}
/* 终态视觉区分：左侧色条 —— 完成绿 / 失败红 / 取消灰 */
.queue-item.is-done {
  border-left-color: #16a34a;
}
.queue-item.is-failed {
  border-left-color: #dc2626;
}
.queue-item.is-cancelled {
  border-left-color: #9ca3af;
}
.queue-item-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}
.queue-progress-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.queue-progress-head .n-progress {
  flex: 1;
}
.queue-progress-text {
  font-size: 12px;
  color: #6b7280;
  white-space: nowrap;
  min-width: 92px;
  text-align: right;
}
.queue-tool {
  font-size: 13px;
  font-weight: 600;
}
/* 终态耗时：弱化但可见 */
.queue-duration {
  font-size: 11px;
  color: #9ca3af;
  white-space: nowrap;
}
.queue-msg {
  font-size: 12px;
  color: #6b7280;
  margin-top: 4px;
  word-break: break-all;
}
/* 失败任务的错误详情：红色提示，一眼可辨 */
.queue-msg.is-error {
  color: #dc2626;
}
.queue-size {
  font-size: 11px;
  color: #6b7280;
  margin-top: 4px;
  white-space: nowrap;
}
.queue-eta {
  color: #16a34a;
}
/* 非下载阶段的脉动进度条（解析/校验/解压） */
.queue-indeterminate {
  position: relative;
  flex: 1;
  height: 6px;
  border-radius: 999px;
  background: #e9ecf1;
  overflow: hidden;
}
.queue-indeterminate-bar {
  position: absolute;
  width: 40%;
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, transparent, #7c6ff0, transparent);
  animation: queue-slide 1.4s ease-in-out infinite;
}
@keyframes queue-slide {
  from {
    left: -40%;
  }
  to {
    left: 100%;
  }
}
/* 下载地址：单行省略 + 展开/复制 */
.queue-url-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
}
.queue-download-url {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1;
  min-width: 0;
  font-size: 11px;
  line-height: 1.5;
  color: #3b82f6;
  text-decoration: none;
}
.queue-url-text {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.queue-download-url.expanded .queue-url-text {
  white-space: normal;
  word-break: break-all;
}
.queue-download-url:hover {
  text-decoration: underline;
}
.queue-download-url .dl-icon {
  flex-shrink: 0;
  font-size: 10px;
  line-height: 1.6;
}
.queue-url-btn {
  flex-shrink: 0;
  padding: 0 4px;
}
</style>
