<script setup lang="ts">
// 设置 —— 通用 / 存储 / 数据管理（导入导出）/ 关于
import { onUnmounted, ref, watch } from "vue";
import { NButton, NInput, NSwitch, NCard, NTooltip } from "naive-ui";
import { useApp, store, showMsg, showErr } from "../store";

const app = useApp();
const newRegistryName = ref("");
const newRegistryUrl = ref("");

// ---- 变更后自动保存（防抖，避免每次输入都写盘） ----
let proxyTimer: ReturnType<typeof setTimeout> | null = null;
let settingsTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleProxySave(delay = 600) {
  if (proxyTimer) clearTimeout(proxyTimer);
  proxyTimer = setTimeout(() => {
    proxyTimer = null;
    void store.saveProxy(true);
  }, delay);
}

function scheduleSettingsSave(delay = 600) {
  if (settingsTimer) clearTimeout(settingsTimer);
  settingsTimer = setTimeout(() => {
    settingsTimer = null;
    void store.saveSettings(true);
  }, delay);
}

watch(() => app.proxyEnable, () => scheduleProxySave(200));
watch(() => app.proxyUrl, () => scheduleProxySave(600));
watch(() => app.cacheTtl, () => scheduleSettingsSave());
watch(() => app.storagePath, () => scheduleSettingsSave());
watch(() => JSON.stringify(app.registryEntries), () => {
  // 存在未填完整的行（缺名字或缺 manifest 地址）时不自动保存，避免打断输入
  if (app.registryEntries.some((e) => !(e.name ?? "").trim() || !(e.url ?? "").trim())) return;
  scheduleSettingsSave();
});

onUnmounted(() => {
  if (proxyTimer) clearTimeout(proxyTimer);
  if (settingsTimer) clearTimeout(settingsTimer);
});

function copyExport() {
  if (!app.exportText) return;
  void navigator.clipboard?.writeText(app.exportText).then(
    () => showMsg("已复制到剪贴板"),
    () => showMsg("复制失败（预览模式下不可用）")
  );
}

function updateRegistryEntry(i: number, key: "name" | "url", v: string) {
  app.registryEntries[i][key] = v;
}

function addRegistryEntry() {
  const name = newRegistryName.value.trim();
  const url = newRegistryUrl.value.trim().replace(/\/+$/, "");
  if (!name) return showMsg("请输入仓库名（如：官方gitee）");
  if (!url) return showMsg("请输入 manifest.json 完整地址（如：https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json）");
  if (app.registryEntries.some((e) => e.url === url)) return showMsg("该仓库地址已存在");
  app.registryEntries.push({ name, url });
  newRegistryName.value = "";
  newRegistryUrl.value = "";
}

function removeRegistryEntry(i: number) {
  if (app.registryEntries.length <= 1) return showErr("至少保留一个插件仓库");
  app.registryEntries.splice(i, 1);
}
</script>

<template>
  <section class="section">
    <!-- ============ 通用 ============ -->
    <n-card size="small" title="通用" class="section-card" :bordered="true">
      <div class="settings-row">
        <span class="proxy-label">开机自启动</span>
        <div style="flex: 1" />
        <n-switch
          :value="app.autostart"
          size="small"
          :loading="app.busy === 'autostart'"
          @update:value="() => store.toggleAutostart()"
        />
      </div>
      <div class="settings-row">
        <span class="proxy-label">关闭窗口时隐藏到系统托盘</span>
        <div style="flex: 1" />
        <n-switch
          :value="app.trayResident"
          size="small"
          :loading="app.busy === 'trayResident'"
          @update:value="() => store.toggleTrayResident()"
        />
      </div>
    </n-card>

    <!-- ============ 下载代理 ============ -->
    <n-card size="small" title="代理" class="section-card" :bordered="true">
      <div class="proxy-label-row">
        <span class="proxy-label">下载代理</span>
        <div style="flex: 1" />
        <n-switch
          :value="app.proxyEnable"
          size="small"
          @update:value="(v: boolean) => (app.proxyEnable = v)"
        />
      </div>
      <div class="proxy-addr-row">
        <label class="proxy-label proxy-addr-label">代理地址</label>
        <n-input
          v-model:value="app.proxyUrl"
          placeholder="http://127.0.0.1:7890"
          style="flex: 1"
        />
      </div>
    </n-card>

    <!-- ============ 存储 ============ -->
    <n-card size="small" title="存储" class="section-card" :bordered="true">
      <div class="settings-form">
        <div class="settings-row">
          <label class="proxy-label ttl-label">
            缓存有效期
            <n-tooltip trigger="hover" placement="top">
              <template #trigger>
                <span class="help-icon">?</span>
              </template>
              <span>拉取的版本列表缓存的保留时长，支持格式：12h（12小时）、3600（秒）、-1（永不过期）、0（禁用缓存）</span>
            </n-tooltip>
          </label>
          <n-input
            v-model:value="app.cacheTtl"
            placeholder="12h / 3600 / -1 / 0"
            style="max-width: 200px"
          />
        </div>
        <div class="settings-row reg-addrs-row">
          <label class="proxy-label" style="min-width: 120px">插件仓库</label>
          <div class="reg-addrs">
            <div v-for="(e, i) in app.registryEntries" :key="i" class="reg-addr-row">
              <n-input
                :value="e.name"
                placeholder="仓库名（如：官方gitee）"
                class="reg-name-input"
                @update:value="(v: string) => updateRegistryEntry(i, 'name', v)"
              />
              <n-input
                :value="e.url"
                placeholder="https://example.com/repo/main/manifest.json"
                @update:value="(v: string) => updateRegistryEntry(i, 'url', v)"
              />
              <n-button size="small" quaternary circle title="删除该仓库" @click="removeRegistryEntry(i)">
                ✕
              </n-button>
            </div>
            <div class="reg-addr-row">
              <n-input
                v-model:value="newRegistryName"
                placeholder="仓库名（如：官方gitee）"
                class="reg-name-input"
              />
              <n-input
                v-model:value="newRegistryUrl"
                placeholder="manifest.json 完整地址（如：https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json）"
                @keyup.enter="addRegistryEntry"
              />
              <n-button size="small" @click="addRegistryEntry">添加</n-button>
            </div>
          </div>
        </div>
        <div class="settings-row">
          <label class="proxy-label" style="min-width: 120px">工具存储路径</label>
          <n-input
            v-model:value="app.storagePath"
            placeholder="~/.envhive/installs"
            style="max-width: 360px"
          />
          <span class="muted">⚠️ 修改需重启生效</span>
        </div>
        <div class="settings-row">
          <span class="muted config-hint">
            配置文件：{{ app.bootstrap?.configFile || "~/.envhive/config.yaml" }}
          </span>
        </div>
      </div>
    </n-card>

    <!-- ============ 数据管理（导入导出） ============ -->
    <n-card size="small" title="数据管理（导入导出）" class="section-card" :bordered="true">
      <div class="envbar-wrap">
        <div class="settings-row">
          <n-button type="primary" size="small" @click="store.doExport('yaml')">导出 YAML</n-button>
          <n-button size="small" @click="store.doExport('json')">导出 JSON</n-button>
          <span class="muted export-hint">导出当前全局工具版本、镜像、代理与环境变量快照</span>
        </div>

        <pre v-if="app.exportText" class="export-box">{{ app.exportText }}</pre>
        <div v-if="app.exportText" class="settings-row">
          <n-button size="small" @click="copyExport">复制导出内容</n-button>
        </div>

        <div class="import-box">
          <n-input
            v-model:value="app.importText"
            type="textarea"
            :rows="5"
            placeholder="粘贴导出的环境快照（YAML / JSON），导入将写全局工具版本、镜像、代理与环境变量"
          />
          <n-button type="primary" size="small" @click="store.doImport()">导入并应用</n-button>
        </div>
      </div>
    </n-card>

    <!-- ============ 关于 ============ -->
    <n-card size="small" title="关于" class="section-card" :bordered="true">
      <span class="mono muted">
        {{
          app.bootstrap
            ? `版本 ${app.bootstrap.version} · ${app.bootstrap.platform}/${app.bootstrap.arch} · 配置 ${app.bootstrap.configWritable ? "可写" : "不可写"} · 安装目录 ${app.bootstrap.installDir}`
            : "加载中…"
        }}
      </span>
    </n-card>
  </section>
</template>

<style scoped>
.section-card {
  margin-bottom: 16px;
}
.section-card :deep(.n-card-header) {
  padding-bottom: 10px;
}
.settings-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
  flex-wrap: wrap;
}
.proxy-label {
  font-size: 13px;
  font-weight: 600;
}
.ttl-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 120px;
}
.help-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #c3c7cd;
  color: #fff;
  font-size: 10px;
  font-weight: 700;
  line-height: 1;
  cursor: help;
  user-select: none;
  transition: background 0.2s;
}
.help-icon:hover {
  background: #8f959e;
}
.config-hint {
  font-size: 11px;
}
.envbar-wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.export-hint {
  font-size: 12px;
}
.export-box {
  background: #f6f7f9;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  padding: 12px 14px;
  font-family: "SFMono-Regular", Consolas, monospace;
  font-size: 12px;
  max-height: 260px;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
.import-box {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
  margin-top: 4px;
}
.proxy-label-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.proxy-addr-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
}
.proxy-desc {
  font-size: 12px;
}
.proxy-addr-label {
  min-width: 56px;
  flex-shrink: 0;
}
.reg-addrs-row {
  align-items: flex-start;
}
.reg-addrs {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
  max-width: 560px;
}
.reg-addr-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.reg-addr-row .n-input {
  flex: 1;
}
.reg-name-input {
  flex: 0 0 150px !important;
}
.reg-name-input .n-input__input-el {
  font-size: 12px;
}
</style>
