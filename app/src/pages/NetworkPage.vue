<script setup lang="ts">
// 镜像源管理 ——工具包镜像源一键切换 + 自定义镜像源
import { ref } from "vue";
import { NButton, NSelect, NInput, NTag, NCard } from "naive-ui";
import { useApp, store } from "../store";
import StatusChip from "../components/StatusChip.vue";
import { REGISTRY_TOOLS, REGISTRY_TOOL_HINT } from "../types";

const app = useApp();

const regConflicts = () => app.conflicts.filter((c) => c.kind === "registry");

// 自定义源添加表单（按工具展开）
const customOpen = ref<Record<string, boolean>>({});
const draft = ref<Record<string, { name: string; url: string }>>({});

function toggleCustom(tool: string) {
  customOpen.value[tool] = !customOpen.value[tool];
}

function presetOptions(tool: string) {
  return (app.presets[tool] ?? []).map((p) => ({
    label: `${p.name}${p.isOfficial ? "（官方）" : ""}${p.isCustom ? "（自定义）" : ""}`,
    value: p.name,
  }));
}

function submitCustom(tool: string) {
  const d = draft.value[tool] ?? { name: "", url: "" };
  if (!d.name.trim() || !d.url.trim()) return;
  void store.addCustomPreset(tool, d.name.trim(), d.url.trim());
  draft.value[tool] = { name: "", url: "" };
  customOpen.value[tool] = false;
}

</script>

<template>
  <section class="section">
    <!-- ============ 镜像源一键切换 ============ -->
    <n-card size="small" title="镜像源一键切换" class="section-card" :bordered="true">
      <p class="muted block-hint">
        每行：工具+ 当前源地址 + 预设下拉 + 状态；可随时追加自定义镜像源，内置官方源自动保留。
      </p>

      <div class="mirror-list">
        <div v-for="tool in REGISTRY_TOOLS" :key="tool" class="mirror-item">
          <!-- 行 1：工具+ 当前源 + 预设下拉 + 操作 -->
          <div class="mirror-row">
            <n-tag size="small" :bordered="false" class="mirror-tool">{{ tool }}</n-tag>
            <span class="mono mirror-url" :title="app.registry[tool]?.currentUrl ?? ''">
              <template v-if="app.registry[tool]?.currentUrl">
                {{ app.registry[tool]?.currentUrl }}
                <StatusChip :tone="app.registry[tool]?.presetName ? 'ok' : 'info'">
                  {{ app.registry[tool]?.presetName ? `✓ ${app.registry[tool]?.presetName}` : "自定义" }}
                </StatusChip>
              </template>
              <span v-else class="muted">未配置</span>
            </span>

            <n-select
              size="small"
              class="mirror-select"
              :value="(app.presets[tool] ?? []).find((p) => p.name === app.registry[tool]?.presetName)?.name ?? ''"
              :options="presetOptions(tool)"
              :placeholder="(app.presets[tool] ?? []).length ? '选择预设…' : '无预设'"
              clearable
              @update:value="(v: string | null) => { if (v) void store.applyPreset(tool, v); }"
            />

            <n-tag
              v-if="regConflicts().find((c) => c.tool === tool)"
              type="error"
              size="small"
              :bordered="false"
              class="mirror-conflict"
            >
              ⚠ 基线不一致
              <n-button
                size="tiny"
                quaternary
                type="error"
                style="margin-left: 4px"
                @click="store.ackConflict(regConflicts().find((c) => c.tool === tool)!)"
              >
                确认基线
              </n-button>
            </n-tag>
          </div>

          <!-- 行 2：配置文件路径 -->
          <div class="mirror-meta">
            <span class="mirror-configfile">
              配置文件：
              <span class="mono" :title="app.registry[tool]?.configFile ?? REGISTRY_TOOL_HINT[tool] ?? ''">
                {{ app.registry[tool]?.configFile ?? REGISTRY_TOOL_HINT[tool] ?? "—" }}
              </span>
            </span>
          </div>

          <!-- 行 3：自定义源管理 -->
          <div class="mirror-custom">
            <span class="mirror-custom-label">自定义源</span>
            <span v-if="(app.presets[tool] ?? []).filter((p) => p.isCustom).length === 0" class="muted">
              暂无，可添加自定义镜像源地址
            </span>
            <template v-else>
              <span
                v-for="c in (app.presets[tool] ?? []).filter((p) => p.isCustom)"
                :key="c.name"
                class="mirror-custom-chip"
                :title="c.url"
              >
                <span class="mirror-custom-name">{{ c.name }}</span>
                <span class="mono mirror-custom-url">{{ c.url }}</span>
                <n-button
                  size="tiny"
                  quaternary
                  type="error"
                  :title="`删除 ${c.name}`"
                  @click="store.removeCustomPreset(tool, c.name)"
                >
                  ✕
                </n-button>
              </span>
            </template>
            <n-button size="tiny" quaternary @click="toggleCustom(tool)">
              {{ customOpen[tool] ? "收起" : "＋ 添加自定义源" }}
            </n-button>

            <div v-if="customOpen[tool]" class="mirror-custom-form">
              <n-input
                size="small"
                :value="draft[tool]?.name ?? ''"
                placeholder="名称，如 my-registry"
                style="width: 140px"
                @update:value="(v: string) => (draft[tool] = { name: v, url: draft[tool]?.url ?? '' })"
              />
              <n-input
                size="small"
                :value="draft[tool]?.url ?? ''"
                placeholder="https://…"
                style="width: 220px"
                @update:value="(v: string) => (draft[tool] = { name: draft[tool]?.name ?? '', url: v })"
              />
              <n-button size="small" type="primary" @click="submitCustom(tool)">添加</n-button>
            </div>
          </div>
        </div>
      </div>
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
.block-hint {
  font-size: 12px;
  margin-bottom: 10px;
}
.mirror-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.mirror-item {
  border: 1px solid #eef0f3;
  border-radius: 10px;
  padding: 10px 14px;
  background: #fff;
}
.mirror-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.mirror-tool {
  min-width: 52px;
  justify-content: center;
}
.mirror-url {
  font-size: 12px;
  flex: 1;
  min-width: 200px;
  display: flex;
  align-items: center;
  gap: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mirror-select {
  width: 180px;
}
.mirror-conflict {
  display: inline-flex;
  align-items: center;
}
.mirror-meta {
  font-size: 11px;
  color: #6b7280;
  margin-top: 6px;
}
.mirror-configfile .mono {
  color: #374151;
}
.mirror-custom {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 6px;
  font-size: 12px;
}
.mirror-custom-label {
  color: #6b7280;
  flex-shrink: 0;
}
.mirror-custom-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: #eeedfe;
  color: #453c9e;
  border-radius: 8px;
  padding: 2px 8px;
  font-size: 12px;
}
.mirror-custom-name {
  font-weight: 600;
}
.mirror-custom-url {
  font-size: 11px;
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mirror-custom-form {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 100%;
  margin-top: 4px;
}
</style>
