<script setup lang="ts">
// 插件 · Lua 管理 —— 插件市场 + 已装插件（来源/发行商/编辑/禁用/删除）+ 抽屉式编辑器
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { NButton, NInput, NSelect, NCard, NDrawer, NDrawerContent, NSwitch, NTooltip } from "naive-ui";
import { useApp, store, LUA_SAMPLE, showMsg } from "../store";
import StatusChip from "../components/StatusChip.vue";
import ToolIcon from "../components/ToolIcon.vue";
import type { ChipTone } from "../components/StatusChip.vue";
import type { PluginInfo, RemotePluginInfo } from "../types";
import { fmtDateTime, relTime } from "../types";

const app = useApp();
const { t } = useI18n();
const editorOpen = ref(false);
/** 编辑模式：非空 = 编辑已有插件（{ name, provider }）；null = 新建 */
const editing = ref<{ name: string; provider: string } | null>(null);
/** 正在执行禁用/启用切换的插件名（开关 loading） */
const toggling = ref("");

const luaPlugins = () => app.plugins.filter((p) => p.provider === "lua");

/** 本地已安装的同名插件（市场卡片判断安装/更新状态用） */
const localPlugin = (name: string) => app.plugins.find((x) => x.name === name);

/** 市场插件存在可更新版本：本地已声明版本号且与市场版本不同（本地未声明版本视为自定义，不提示更新） */
const hasUpdate = (p: RemotePluginInfo) => {
  const l = localPlugin(p.name);
  return !!l?.version && l.version !== p.version;
};

/** 插件市场切换下拉选项：label = 仓库名，value = manifest.json 完整地址 */
const registryOptions = computed(() =>
  app.registryEntries.map((e) => ({ label: e.name, value: e.url }))
);

/** 来源徽标元数据（内置插件已移除，仅剩 market / local）；用函数取词以随语言切换刷新 */
const srcMeta = (s: string) => {
  const map: Record<string, { tone: ChipTone; label: string }> = {
    market: { tone: "info", label: t("plugins.source.market") },
    local: { tone: "muted", label: t("plugins.source.local") },
  };
  return map[s] ?? { tone: "muted" as ChipTone, label: s };
};

/** 插件定义文件类型徽标（全部为 Lua 脚本） */
const providerMeta = (_p: string) => ({ tone: "accent" as ChipTone, label: t("plugins.provider.lua") });

/** 压缩插件路径显示：把用户目录段替换为 `~`（跨平台兼容 Windows `\` / Unix `/`） */
function shortPath(p: string): string {
  const idx = Math.max(p.indexOf("\\.envhive\\"), p.indexOf("/.envhive/"));
  return idx > 0 ? "~" + p.slice(idx) : p;
}

function openNew() {
  app.luaName = "";
  app.luaScript = LUA_SAMPLE;
  editing.value = null;
  editorOpen.value = true;
}

/** 编辑/查看已有插件：读取源码 → 填入编辑器 */
async function openEdit(p: PluginInfo) {
  const src = await store.loadPluginSource(p.name);
  if (!src) return;
  app.luaName = src.name;
  app.luaScript = src.script;
  editing.value = { name: src.name, provider: src.provider };
  editorOpen.value = true;
}

function doSave() {
  if (editing.value) {
    void store.saveLuaPlugin(editing.value.name, editing.value.provider, app.luaScript).then(() => {
      editorOpen.value = false;
      editing.value = null;
    });
  } else {
    void store.addLuaPlugin().then(() => {
      editorOpen.value = false;
    });
  }
}

/** 禁用/启用开关：点击后置 loading，失败自动回弹（开关绑定 app.plugins 状态） */
function doToggle(p: PluginInfo, enable: boolean) {
  if (toggling.value) return;
  toggling.value = p.name;
  store.togglePlugin(p.name, enable).finally(() => (toggling.value = ""));
}
</script>

<template>
  <section class="section">
    <!-- ============ 插件市场 ============ -->
    <n-card size="small" :title="t('plugins.marketTitle')" class="section-card" :bordered="true">
      <template #header-extra>
        <n-select
          v-if="app.registryEntries.length > 1"
          :value="app.remoteRegistry"
          :options="registryOptions"
          size="small"
          class="reg-switch"
          :loading="app.remotePluginsLoading"
          @update:value="(v: string) => store.switchRemoteRegistry(v)"
        />
        <span v-else-if="app.registryEntries.length === 1" class="muted reg-single">
          {{ app.registryEntries[0].name }}
        </span>
      </template>
      <p v-if="app.remotePluginsLoading" class="muted">{{ t("plugins.loading") }}</p>
      <p v-else-if="app.remotePluginsError" class="muted reg-err">
        {{ t("plugins.loadFailed", { msg: app.remotePluginsError }) }}
      </p>
      <p v-else-if="app.remotePlugins.length === 0" class="muted">
        {{ t("plugins.emptyMarket") }}
      </p>
      <div v-else class="reg-grid">
        <div v-for="(p, i) in app.remotePlugins" :key="p.name" class="card reg-card">
          <div class="card-head">
            <ToolIcon :icon="p.icon" :index="i" :size="18" :name="p.name" />
            <span class="card-name">{{ p.name }}</span>
            <span class="mono">v{{ p.version }}</span>
            <StatusChip v-if="!localPlugin(p.name)" tone="info">{{ t("plugins.notInstalled") }}</StatusChip>
            <StatusChip v-else-if="hasUpdate(p)" tone="warn" :title="t('plugins.updatableTip')">{{ t("plugins.updatable") }}</StatusChip>
            <StatusChip v-else tone="ok">{{ t("plugins.installedCheck") }}</StatusChip>
          </div>
          <p class="muted reg-desc">{{ p.description || t("common.noDescription") }}</p>
          <div class="card-foot">
            <n-button v-if="!localPlugin(p.name)" type="primary" size="small" @click="store.installRemotePlugin(p)">
              {{ t("common.install") }}
            </n-button>
            <n-button v-else-if="hasUpdate(p)" type="primary" size="small" @click="store.installRemotePlugin(p)">
              {{ t("plugins.updateTo", { version: p.version }) }}
            </n-button>
            <span v-else class="muted">
              {{
                localPlugin(p.name)?.version
                  ? t("plugins.installedWithVersion", { version: localPlugin(p.name)!.version })
                  : t("plugins.installedPlain")
              }}
            </span>
          </div>
        </div>
      </div>
    </n-card>

    <!-- ============ 已安装插件 ============ -->
    <n-card size="small" :title="t('plugins.installedTitle')" class="section-card" :bordered="true">
      <template #header-extra>
        <n-button size="small" quaternary style="margin-right: 8px" @click="store.loadP2()">{{ t("common.refresh") }}</n-button>
        <n-button type="primary" size="small" @click="openNew">{{ t("plugins.newPlugin") }}</n-button>
      </template>
      <p v-if="app.plugins.length === 0" class="muted">
        {{ t("plugins.emptyInstalled") }}
      </p>
      <div v-else class="installed-plugin-list">
        <div v-for="(p, i) in app.plugins" :key="p.name" class="installed-plugin-row" :class="{ 'row-disabled': !p.enabled }">
          <!-- 主行：身份 + 操作 -->
          <div class="row-main">
            <div class="row-id">
              <ToolIcon :icon="p.icon" :index="i" :size="18" :name="p.display" />
              <span class="card-name">{{ p.display }}</span>
              <span class="mono muted">{{ p.name }}</span>
              <span v-if="p.version" class="mono muted small">v{{ p.version }}</span>
            </div>
            <div class="row-ops">
              <n-tooltip placement="top">
                <template #trigger>
                  <n-button size="tiny" quaternary @click="store.openPluginDir(p.name)">{{ t("common.openDir") }}</n-button>
                </template>
                {{ p.path }}
              </n-tooltip>
              <n-button
                size="tiny"
                quaternary
                :disabled="!p.enabled"
                :title="p.enabled ? t('plugins.editTitleEnabled') : t('plugins.editTitleDisabled')"
                @click="openEdit(p)"
              >
                {{ t("common.edit") }}
              </n-button>
              <n-tooltip placement="top" :show-arrow="false">
                <template #trigger>
                  <n-switch
                    :value="p.enabled"
                    size="small"
                    :loading="toggling === p.name"
                    :disabled="toggling !== '' && toggling !== p.name"
                    @update:value="(v: boolean) => doToggle(p, v)"
                  />
                </template>
                {{ p.enabled ? t("plugins.toggleDisable") : t("plugins.toggleEnable") }}
              </n-tooltip>
              <n-button
                size="tiny"
                quaternary
                type="error"
                :title="t('plugins.deleteTitle', { name: p.name })"
                @click="store.deletePlugin(p.name, p.display)"
              >
                {{ t("common.delete") }}
              </n-button>
            </div>
          </div>

          <!-- 元行：属性 + 元数据 -->
          <div class="row-meta">
            <StatusChip :tone="srcMeta(p.source).tone">{{ srcMeta(p.source).label }}</StatusChip>
            <StatusChip :tone="providerMeta(p.provider).tone">{{ providerMeta(p.provider).label }}</StatusChip>
            <StatusChip :tone="p.enabled ? 'ok' : 'warn'">{{ p.enabled ? t("plugins.enabled") : t("plugins.disabled") }}</StatusChip>
            <n-tooltip v-if="p.distributions.length > 0" placement="top">
              <template #trigger>
                <StatusChip tone="info">{{ t("plugins.distributionsCount", { count: p.distributions.length }) }}</StatusChip>
              </template>
              {{ p.distributions.map((d) => d.display).join(" · ") }}
            </n-tooltip>
          </div>
          <div class="row-meta">
            <n-tooltip placement="top">
              <template #trigger>
                <span class="meta-text">
                  <template v-if="p.installedVersions.length > 0">
                    {{ t("plugins.downloadedVersions", { count: p.installedVersions.length }) }}
                  </template>
                  <template v-else>
                    {{ t("plugins.notDownloaded") }}
                  </template>
                </span>
              </template>
              {{ p.installedVersions.join(" · ") }}
            </n-tooltip>
            <template v-if="p.updatedAt">
              <span class="meta-sep">·</span>
              <n-tooltip placement="top">
                <template #trigger>
                  <span class="meta-text">{{ t("plugins.updatedAt", { time: relTime(p.updatedAt) }) }}</span>
                </template>
                {{ fmtDateTime(p.updatedAt) }}
              </n-tooltip>
            </template>
            <span class="meta-sep">·</span>
            <n-tooltip placement="top">
              <template #trigger>
                <span class="meta-path mono">{{ shortPath(p.path) }}</span>
              </template>
              {{ p.path }}
            </n-tooltip>
          </div>
          
        </div>
      </div>
      <p v-if="luaPlugins().length > 0" class="muted lua-dir-hint">
        {{ t("plugins.luaDirHint") }}
      </p>
    </n-card>

    <!-- ============ 抽屉式编辑器 ============ -->
    <n-drawer v-model:show="editorOpen" :width="560" placement="right">
      <n-drawer-content :title="editing ? t('plugins.editorEditTitle', { name: editing.name }) : t('plugins.editorNewTitle')" closable>
        <div class="vcp-row" style="margin-bottom: 8px">
          <n-input
            v-model:value="app.luaName"
            :placeholder="t('plugins.pluginNamePlaceholder')"
            style="flex: 1"
            :disabled="!!editing"
          />
          <n-tooltip v-if="editing" placement="top">
            <template #trigger>
              <StatusChip tone="accent">{{ t("plugins.provider.lua") }}</StatusChip>
            </template>
            {{ t("plugins.luaEditableTip") }}
          </n-tooltip>
        </div>

        <n-input
          v-model:value="app.luaScript"
          type="textarea"
          :rows="14"
          :placeholder="LUA_SAMPLE"
          class="lua-box"
          spellcheck="false"
        />

        <p class="muted lua-hint">
          {{ t("plugins.luaHint") }}
        </p>

        <template #footer>
          <div class="drawer-foot">
            <n-button
              @click="showMsg(app.luaScript.trim() ? t('plugins.dryRunNonEmpty') : t('plugins.dryRunEmpty'))"
            >
              {{ t("plugins.dryRun") }}
            </n-button>
            <n-button type="primary" @click="doSave">
              {{ editing ? t("plugins.saveChanges") : t("plugins.savePlugin") }}
            </n-button>
          </div>
        </template>
      </n-drawer-content>
    </n-drawer>
  </section>
</template>

<style scoped>
.section-card {
  margin-bottom: 16px;
}
.section-card :deep(.n-card-header) {
  padding-bottom: 10px;
}
.reg-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 12px;
}
.reg-switch {
  width: 300px;
}
.reg-single {
  font-size: 12px;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  display: inline-block;
  vertical-align: middle;
}
.reg-err {
  color: #d4537e;
}
.card {
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  padding: 14px 16px;
}
.card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}
.card-name {
  font-weight: 700;
  font-size: 14px;
}
.reg-desc {
  font-size: 12px;
  margin-bottom: 10px;
}
.card-foot {
  display: flex;
  justify-content: flex-end;
}
.installed-plugin-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.installed-plugin-row {
  border: 1px solid #eef0f3;
  border-radius: 10px;
  padding: 10px 14px;
  background: #fff;
}
.installed-plugin-row.row-disabled {
  opacity: 0.55;
}

/* 主行：身份 + 操作 */
.row-main {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 6px;
}
.row-id {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}
.row-ops {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

/* 元行：属性 + 元数据（弱化，可换行） */
.row-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 12px;
  font-size: 12px;
  color: #6b7280;
  padding-left: 2px;
}
.meta-sep {
  color: #d1d5db;
}
.meta-text {
  cursor: default;
}
.meta-path {
  font-size: 11px;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.small {
  font-size: 12px;
}
.lua-dir-hint {
  font-size: 12px;
  margin-top: 8px;
}
.vcp-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.lua-box {
  font-family: "SFMono-Regular", Consolas, monospace;
  font-size: 12px;
}
.lua-hint {
  font-size: 11px;
  margin-top: 8px;
}
.drawer-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
