<script setup lang="ts">
// 插件 · Lua 管理 —— 插件市场 + 已装插件（来源/发行商/编辑/禁用/删除）+ 抽屉式编辑器
import { computed, ref } from "vue";
import { NButton, NInput, NSelect, NCard, NDrawer, NDrawerContent, NSwitch, NTooltip } from "naive-ui";
import { useApp, store, LUA_SAMPLE, showMsg } from "../store";
import StatusChip from "../components/StatusChip.vue";
import ToolIcon from "../components/ToolIcon.vue";
import type { ChipTone } from "../components/StatusChip.vue";
import type { PluginInfo, RemotePluginInfo } from "../types";
import { fmtDateTime, relTime } from "../types";

const app = useApp();
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

/** 来源徽标元数据（内置插件已移除，仅剩 market / local） */
const SRC_META: Record<string, { tone: ChipTone; label: string }> = {
  market: { tone: "info", label: "市场" },
  local: { tone: "muted", label: "本地" },
};
const srcMeta = (s: string) => SRC_META[s] ?? { tone: "muted" as ChipTone, label: s };

/** 插件定义文件类型徽标（全部为 Lua 脚本） */
const providerMeta = (_p: string) => ({ tone: "accent" as ChipTone, label: "Lua 脚本" });

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
    <n-card size="small" title="插件市场（远程仓库）" class="section-card" :bordered="true">
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
      <p v-if="app.remotePluginsLoading" class="muted">正在获取插件列表…</p>
      <p v-else-if="app.remotePluginsError" class="muted reg-err">
        获取插件列表失败：{{ app.remotePluginsError }}
      </p>
      <p v-else-if="app.remotePlugins.length === 0" class="muted">
        该仓库暂无可用插件（可在设置中配置多个插件仓库地址）。
      </p>
      <div v-else class="reg-grid">
        <div v-for="(p, i) in app.remotePlugins" :key="p.name" class="card reg-card">
          <div class="card-head">
            <ToolIcon :icon="p.icon" :index="i" :size="18" :name="p.name" />
            <span class="card-name">{{ p.name }}</span>
            <span class="mono">v{{ p.version }}</span>
            <StatusChip v-if="!localPlugin(p.name)" tone="info">未安装</StatusChip>
            <StatusChip v-else-if="hasUpdate(p)" tone="warn" title="本地已装版本与市场版本不同，可一键更新">可更新</StatusChip>
            <StatusChip v-else tone="ok">已安装 ✓</StatusChip>
          </div>
          <p class="muted reg-desc">{{ p.description || "（无描述）" }}</p>
          <div class="card-foot">
            <n-button v-if="!localPlugin(p.name)" type="primary" size="small" @click="store.installRemotePlugin(p)">
              安装
            </n-button>
            <n-button v-else-if="hasUpdate(p)" type="primary" size="small" @click="store.installRemotePlugin(p)">
              更新到 v{{ p.version }}
            </n-button>
            <span v-else class="muted">
              已安装
              <template v-if="localPlugin(p.name)?.version"> v{{ localPlugin(p.name)!.version }}</template>
              ，可在工具列表中使用
            </span>
          </div>
        </div>
      </div>
    </n-card>

    <!-- ============ 已安装插件 ============ -->
    <n-card size="small" title="已安装插件" class="section-card" :bordered="true">
      <template #header-extra>
        <n-button size="small" quaternary style="margin-right: 8px" @click="store.loadP2()">刷新</n-button>
        <n-button type="primary" size="small" @click="openNew">新建插件</n-button>
      </template>
      <p v-if="app.plugins.length === 0" class="muted">
        暂无插件 —— 应用不再内置插件，全部从 Git 仓库（上方「插件市场」）下载安装；
        启动时已自动同步，可点击右上角「刷新」或直接安装市场中的插件。
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
                  <n-button size="tiny" quaternary @click="store.openPluginDir(p.name)">打开目录</n-button>
                </template>
                {{ p.path }}
              </n-tooltip>
              <n-button
                size="tiny"
                quaternary
                :disabled="!p.enabled"
                :title="p.enabled ? '编辑脚本（校验后原子写回）' : '已禁用插件不可编辑，请先启用'"
                @click="openEdit(p)"
              >
                编辑
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
                {{ p.enabled ? "禁用插件" : "启用插件" }}
              </n-tooltip>
              <n-button
                size="tiny"
                quaternary
                type="error"
                :title="'删除插件 ' + p.name + '（仅删定义，已装版本保留）'"
                @click="store.deletePlugin(p.name, p.display)"
              >
                删除
              </n-button>
            </div>
          </div>

          <!-- 元行：属性 + 元数据 -->
          <div class="row-meta">
            <StatusChip :tone="srcMeta(p.source).tone">{{ srcMeta(p.source).label }}</StatusChip>
            <StatusChip :tone="providerMeta(p.provider).tone">{{ providerMeta(p.provider).label }}</StatusChip>
            <StatusChip :tone="p.enabled ? 'ok' : 'warn'">{{ p.enabled ? "启用" : "已禁用" }}</StatusChip>
            <n-tooltip v-if="p.distributions.length > 0" placement="top">
              <template #trigger>
                <StatusChip tone="info">{{ p.distributions.length }} 个发行商</StatusChip>
              </template>
              {{ p.distributions.map((d) => d.display).join(" · ") }}
            </n-tooltip>
          </div>
          <div class="row-meta">
            <n-tooltip placement="top">
              <template #trigger>
                <span class="meta-text">
                  <template v-if="p.installedVersions.length > 0">
                    已下载 {{ p.installedVersions.length }} 个版本
                  </template>
                  <template v-else>
                    未下载
                  </template>
                </span>
              </template>
              {{ p.installedVersions.join(" · ") }}
            </n-tooltip>
            <template v-if="p.updatedAt">
              <span class="meta-sep">·</span>
              <n-tooltip placement="top">
                <template #trigger>
                  <span class="meta-text">更新于 {{ relTime(p.updatedAt) }}</span>
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
        Lua 插件目录：~/.envhive/plugins/&lt;name&gt;/plugin.lua（可直接修改文件）
      </p>
    </n-card>

    <!-- ============ 抽屉式编辑器 ============ -->
    <n-drawer v-model:show="editorOpen" :width="560" placement="right">
      <n-drawer-content :title="editing ? `编辑插件：${editing.name}` : '新建 Lua 插件'" closable>
        <div class="vcp-row" style="margin-bottom: 8px">
          <n-input
            v-model:value="app.luaName"
            placeholder="插件名（如 python）"
            style="flex: 1"
            :disabled="!!editing"
          />
          <n-tooltip v-if="editing" placement="top">
            <template #trigger>
              <StatusChip tone="accent">Lua 脚本</StatusChip>
            </template>
            可编辑，保存时校验后原子写回
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
          生命周期 hook：available(ctx) / pre_install(ctx) / post_install(ctx) / env_keys(ctx) / pre_uninstall(ctx)；
          内置模块 http.get（NETWORK_ALLOW 白名单）、json、archiver.extract、file（仅限 ~/.envhive 与临时目录）、
          versions.parse（SDKMAN 风格标识符解析）；插件 lib/ 子目录可 require 私有模块。
        </p>

        <template #footer>
          <div class="drawer-foot">
            <n-button
              @click="showMsg(app.luaScript.trim() ? '脚本非空，可保存（完整 dry-run 校验待后端 plugin_validate 接入）' : '脚本为空')"
            >
              测试运行（dry-run）
            </n-button>
            <n-button type="primary" @click="doSave">
              {{ editing ? "保存更改" : "保存插件" }}
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
