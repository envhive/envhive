<script setup lang="ts">
// 统计 —— 使用统计柱状条 + 存储占用（按工具树形分组）
import { computed, h } from "vue";
import { useI18n } from "vue-i18n";
import {
  NButton,
  NDataTable,
  NProgress,
  NCard,
  type DataTableColumns,
} from "naive-ui";
import { useApp, store } from "../store";
import StatusChip from "../components/StatusChip.vue";
import ToolIcon from "../components/ToolIcon.vue";
import EmptyState from "../components/EmptyState.vue";
import { fmtBytes, type ToolInfo } from "../types";

const app = useApp();
const { t } = useI18n();

const maxCount = computed(() =>
  app.usageStats ? Math.max(1, ...app.usageStats.toolUsage.map((s) => s.totalCount)) : 1
);

// ---------- 存储占用：树形表格 ----------
interface UsageRow {
  key: string;
  /**工具内部名（用于卸载/定位 ToolInfo） */
  tool: string;
  display: string;
  /**工具图标 data URI（无图标回退彩色圆点） */
  icon?: string | null;
  /** 列表顺序索引（无图标时圆点取色） */
  idx: number;
  /** 父节点为 null；子节点为具体版本号 */
  version: string | null;
  count: number | null;
  lastUsedDaysAgo: number | null;
  diskBytes: number;
  isCurrent: boolean;
  installed: boolean;
  children?: UsageRow[];
}

const treeData = computed<UsageRow[]>(() =>
  (app.usageStats?.toolUsage ?? []).map((s, i) => ({
    key: s.tool,
    tool: s.tool,
    display: s.display,
    icon: s.icon,
    idx: i,
    version: null,
    count: s.totalCount,
    lastUsedDaysAgo: null,
    diskBytes: s.diskBytes,
    isCurrent: false,
    installed: true,
    children: s.versions.map((v) => ({
      key: `${s.tool}@${v.version}`,
      tool: s.tool,
      display: s.display,
      icon: s.icon,
      idx: i,
      version: v.version,
      count: v.count,
      lastUsedDaysAgo: v.lastUsedDaysAgo,
      diskBytes: v.diskBytes,
      isCurrent: v.isCurrent,
      installed: v.installed,
    })),
  }))
);

/** 卸载单个版本（非当前激活才展示删除入口） */
async function removeVersion(row: UsageRow) {
  if (!row.version) return;
  const tool: ToolInfo =
    app.tools.find((x) => x.name === row.tool) ?? {
      name: row.tool,
      display: row.display,
      category: "",
      homepage: "",
      current: null,
      installed: [],
      available: null,
      binPath: null,
    };
  await store.uninstallVersion(tool, row.version);
}

const columns = computed<DataTableColumns<UsageRow>>(() => [
  {
    title: t("stats.colTool"),
    key: "tool",
    render: (row) =>
      row.version
        ? ""
        : h("span", { class: "cell-name-wrap" }, [
            h(ToolIcon, { icon: row.icon, index: row.idx, size: 16, name: row.display }),
            h("span", { class: "cell-name" }, row.display),
          ]),
  },
  {
    title: t("stats.colVersion"),
    key: "version",
    render: (row) =>
      row.version
        ? h("span", { class: "mono" }, row.version)
        : h("span", { class: "muted" }, t("stats.allVersions", { count: row.children?.length ?? 0 })),
  },
  {
    title: t("stats.colCount"),
    key: "count",
    width: 90,
    render: (row) => (row.version ? (row.count || "—") : row.count),
  },
  {
    title: t("stats.colLastUsed"),
    key: "lastUsed",
    render: (row) =>
      row.version
        ? row.lastUsedDaysAgo === null
          ? t("stats.neverUsed")
          : t("stats.daysAgo", { n: row.lastUsedDaysAgo })
        : "—",
  },
  {
    title: t("stats.colDisk"),
    key: "disk",
    render: (row) =>
      h("span", { class: row.version ? "mono" : "mono cell-disk" }, fmtBytes(row.diskBytes)),
  },
  {
    title: t("stats.colStatus"),
    key: "status",
    render: (row) =>
      row.version
        ? h(
            StatusChip,
            { tone: row.isCurrent ? "accent" : row.installed ? "ok" : "muted" },
            {
              default: () =>
                row.isCurrent
                  ? t("stats.currentStar")
                  : row.installed
                    ? t("common.installed")
                    : t("stats.notInstalled"),
            }
          )
        : null,
  },
  {
    title: t("stats.colOps"),
    key: "actions",
    width: 90,
    render: (row) =>
      row.version && !row.isCurrent
        ? h(
            NButton,
            {
              size: "small",
              quaternary: true,
              type: "error",
              loading: app.busy === row.tool,
              onClick: () => removeVersion(row),
            },
            { default: () => t("common.delete") }
          )
        : null,
  },
]);
</script>

<template>
  <section class="section">
    <EmptyState
      v-if="!app.usageStats"
      :hint="t('stats.previewHint')"
    />

    <template v-else>
      <!-- ============ 使用统计 ============ -->
      <n-card size="small" :title="t('stats.usageTitle')" class="section-card" :bordered="true">
        <div class="stats-bars">
          <div v-for="(s, i) in app.usageStats.toolUsage" :key="s.tool" class="stats-bar-row">
            <span class="stats-bar-name">
              <ToolIcon :icon="s.icon" :index="i" :size="16" :name="s.display" />
              {{ s.display }}
            </span>
            <span class="stats-bar-track">
              <n-progress
                type="line"
                :percentage="Math.max(4, (s.totalCount / maxCount) * 100)"
                :height="10"
                :show-indicator="false"
                color="#534ab7"
              />
            </span>
            <span class="stats-bar-count">{{ t("stats.times", { count: s.totalCount }) }}</span>
          </div>
        </div>
        <p class="muted stats-summary">
          {{ t("stats.summary", { versions: app.usageStats.totalVersions, disk: fmtBytes(app.usageStats.totalDiskBytes) }) }}
        </p>
      </n-card>

      <!-- ============ 存储占用（树形分组） ============ -->
      <n-card size="small" :title="t('stats.storageTitle')" class="section-card" :bordered="true">
        <n-data-table
          :columns="columns"
          :data="treeData"
          :row-key="(row: UsageRow) => row.key"
          :bordered="false"
          size="small"
          single-line
        />
        <p class="muted stats-summary">{{ t("stats.totalDisk", { disk: fmtBytes(app.usageStats.totalDiskBytes) }) }}</p>
      </n-card>
    </template>
  </section>
</template>

<style scoped>
.section-card {
  margin-bottom: 16px;
}
.section-card :deep(.n-card-header) {
  padding-bottom: 10px;
}
.stats-bars {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.stats-bar-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.stats-bar-name {
  font-size: 13px;
  font-weight: 600;
  min-width: 100px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.stats-bar-track {
  flex: 1;
}
.stats-bar-count {
  font-size: 12px;
  color: #6b7280;
  min-width: 56px;
  text-align: right;
}
.stats-summary {
  font-size: 12px;
  margin-top: 8px;
}
.section-card :deep(.cell-name-wrap) {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  vertical-align: -0.2em;
}
.section-card :deep(.cell-name) {
  font-weight: 600;
  font-size: 13px;
}
.section-card :deep(.cell-disk) {
  font-weight: 600;
}
</style>
