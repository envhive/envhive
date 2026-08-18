<script setup lang="ts">
// QueueDrawer —— 全局悬浮队列抽屉（右下角 FAB + 抽屉）
// v2 优化：耗时展示 / 失败+取消可重试 / 全部取消+清空已完成 / ETA / 阶段脉动条 / URL 折叠复制 / FAB 空队列隐藏
import { computed, reactive } from "vue";
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
import {
  STAGE_TEXT,
  TASK_STATUS_TEXT,
  fmtSpeed,
  fmtBytes,
  fmtDuration,
  fmtDurationSec,
  fmtDateTime,
} from "../types";
import type { QueueTask, QueueEnqueueResult } from "../types";

const app = useApp();

const runningCount = computed(
  () => app.queue.filter((t) => t.status === "running" || t.status === "queued").length
);

const hasFinished = computed(() =>
  app.queue.some((t) => t.status === "done" || t.status === "failed" || t.status === "cancelled")
);

const statusType: Record<QueueTask["status"], "info" | "success" | "error" | "warning" | "default"> = {
  queued: "info",
  running: "warning",
  done: "success",
  failed: "error",
  cancelled: "default",
};

// 运行中任务显示实时阶段文本（合并 download-progress 回填的 stage），其余显示任务状态
function statusLabel(t: QueueTask): string {
  if (t.status === "running" && t.stage) return STAGE_TEXT[t.stage] ?? "执行中";
  return TASK_STATUS_TEXT[t.status];
}

// 进度摘要：下载中显示 百分比 + 速度（<1MB/s 自动转 KB/s）；其余阶段只显示百分比
function progressText(t: QueueTask): string {
  const pct = `${Math.max(0, Math.min(100, t.percent)).toFixed(0)}%`;
  if (t.stage === "downloading" && t.speedMbps != null) {
    return `${pct} · ${fmtSpeed(t.speedMbps)}`;
  }
  return pct;
}

// ETA：下载中且大小/速度齐全时，按 (剩余字节 / 速度) 估算剩余时间
function etaText(t: QueueTask): string {
  if (t.stage !== "downloading" || t.speedMbps == null || t.speedMbps <= 0) return "";
  if (t.totalBytes == null || t.downloadedBytes == null) return "";
  const remain = (t.totalBytes - t.downloadedBytes) / (t.speedMbps * 1024 * 1024);
  if (remain <= 0) return "";
  return `剩余 ${fmtDurationSec(remain)}`;
}

// 终态任务耗时（createdAt → finishedAt）；非终态返回空串
function durationText(t: QueueTask): string {
  return fmtDuration(t.createdAt, t.finishedAt);
}

// ---- 取消：乐观禁用，防止重复点击 ----
const cancellingIds = reactive(new Set<number>());
async function cancelTask(t: QueueTask) {
  if (cancellingIds.has(t.id)) return;
  cancellingIds.add(t.id);
  try {
    await store.cancelTask(t.id);
  } catch {
    /* store 内已提示 */
  } finally {
    cancellingIds.delete(t.id);
  }
}

// ---- 重试：失败 / 已取消 均可重新入队（后端按 tool+version+distribution 去重） ----
async function retry(t: QueueTask) {
  try {
    const r = await store.run<QueueEnqueueResult>("enqueue_install", {
      name: t.tool,
      version: t.version,
      distribution: t.distribution ?? null,
    });
    if (r.reused) {
      showMsg(`${t.tool} ${t.version} 已在队列中（任务 #${r.id}）`);
    } else {
      showMsg(`${t.tool} ${t.version} 已重新入队（任务 #${r.id}）`);
    }
  } catch (e) {
    showErr(`重试失败：${errMsg(e)}`);
  }
}

// ---- 下载地址：折叠（单行省略）/ 展开 + 复制 ----
const expandedUrls = reactive(new Set<number>());
function toggleUrl(t: QueueTask) {
  if (expandedUrls.has(t.id)) expandedUrls.delete(t.id);
  else expandedUrls.add(t.id);
}
async function copyUrl(url: string) {
  try {
    await navigator.clipboard.writeText(url);
    showMsg("下载链接已复制");
  } catch {
    showErr("复制失败，请手动选择复制");
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
          <span class="queue-title">下载队列</span>
          <span class="queue-count muted">
            {{ app.queue.length > 0 ? `${app.queue.length} 个任务 · 进行中 ${runningCount}` : "暂无任务" }}
          </span>
          <div style="flex: 1" />
          <n-button v-if="runningCount > 0" size="tiny" quaternary type="error" @click="cancelAll">
            全部取消
          </n-button>
          <n-button v-if="hasFinished" size="tiny" quaternary @click="clearFinished">
            清空已完成
          </n-button>
        </div>
      </template>

      <div v-if="app.queue.length === 0" class="muted queue-empty">
        暂无任务。在「工具管理」中选择版本并点击安装后，任务会出现在这里。
      </div>

      <div v-else class="queue-list">
        <div v-for="t in app.queue" :key="t.id" class="queue-item" :class="`is-${t.status}`">
          <div class="queue-item-head">
            <n-tag :type="statusType[t.status]" size="small" round>
              {{ statusLabel(t) }}
            </n-tag>
            <span class="queue-tool">
              {{ t.tool }} {{ t.version }}
              <span v-if="t.distribution" class="mono muted">· {{ t.distribution }}</span>
            </span>
            <!-- 终态耗时（createdAt → finishedAt） -->
            <span
              v-if="durationText(t)"
              class="queue-duration mono muted"
              :title="`入队 ${fmtDateTime(t.createdAt)}`"
            >
              ⏱ {{ durationText(t) }}
            </span>
            <div style="flex: 1" />
            <n-button
              v-if="t.status === 'queued' || t.status === 'running'"
              size="tiny"
              quaternary
              type="error"
              :loading="cancellingIds.has(t.id)"
              :disabled="cancellingIds.has(t.id)"
              @click="cancelTask(t)"
            >
              {{ t.status === "running" ? "取消下载" : "取消" }}
            </n-button>
            <n-button
              v-if="t.status === 'failed' || t.status === 'cancelled'"
              size="tiny"
              quaternary
              @click="retry(t)"
            >
              重试
            </n-button>
          </div>

          <template v-if="t.status === 'running'">
            <!-- 下载中：真实进度条 + 百分比/速度；其余阶段（解析/校验/解压）：脉动条（indeterminate） -->
            <div v-if="t.stage === 'downloading'" class="queue-progress-head">
              <n-progress
                type="line"
                :percentage="Math.max(0, Math.min(100, t.percent))"
                :height="6"
                :show-indicator="false"
                status="success"
              />
              <span class="queue-progress-text mono">{{ progressText(t) }}</span>
            </div>
            <div v-else class="queue-progress-head">
              <div class="queue-indeterminate" role="progressbar" aria-label="任务执行中">
                <div class="queue-indeterminate-bar" />
              </div>
              <span class="queue-progress-text mono">
                {{ t.stage ? STAGE_TEXT[t.stage] : "执行中…" }}
              </span>
            </div>
            <!-- 下载中：已下载 / 总大小 + ETA（总大小未知时只显示已下载） -->
            <div v-if="t.stage === 'downloading'" class="queue-size mono">
              <template v-if="t.totalBytes != null">
                {{ fmtBytes(t.downloadedBytes ?? 0) }} / {{ fmtBytes(t.totalBytes) }}
              </template>
              <template v-else>已下载 {{ fmtBytes(t.downloadedBytes ?? 0) }}</template>
              <span v-if="etaText(t)" class="queue-eta">· {{ etaText(t) }}</span>
            </div>
            <!-- 下载中：实际下载地址（默认单行省略，可展开；支持一键复制） -->
            <div v-if="t.stage === 'downloading' && t.url" class="queue-url-row">
              <a
                class="queue-download-url mono"
                :class="{ expanded: expandedUrls.has(t.id) }"
                :href="t.url"
                target="_blank"
                rel="noreferrer"
                :title="t.url"
              >
                <span class="dl-icon">🔗</span>
                <span class="queue-url-text">{{ t.url }}</span>
              </a>
              <n-button size="tiny" quaternary class="queue-url-btn" @click="toggleUrl(t)">
                {{ expandedUrls.has(t.id) ? "收起" : "展开" }}
              </n-button>
              <n-button size="tiny" quaternary class="queue-url-btn" @click="copyUrl(t.url)">
                复制
              </n-button>
            </div>
          </template>
          <div
            v-if="t.message"
            class="queue-msg mono"
            :class="{ 'is-error': t.status === 'failed' }"
          >
            {{ t.message }}
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
