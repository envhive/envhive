<script setup lang="ts">
import { NButton, NTag } from "naive-ui";
import { useApp, store } from "../store";
import ToolIcon from "./ToolIcon.vue";
import type { ProjectPreset } from "../types";

const props = defineProps<{
  preset: ProjectPreset;
}>();

const emit = defineEmits<{ edit: [p: ProjectPreset] }>();

const app = useApp();

// TODO: 复制脚本功能暂缓（按需恢复）
// function copyScript() {
//   const lines = props.preset.versions.map((v) => `:: ${v.tool} ${v.version}`).join("\n");
//   const script = `:: EnvHive 项目环境注入（${props.preset.name}）\n${lines}\n:: 完整注入脚本请安装 envhive-cli 后使用 envhive run`;
//   void navigator.clipboard?.writeText(script).then(
//     () => showMsg("脚本已复制到剪贴板（命令行注入需安装 envhive-cli）"),
//     () => showMsg("复制失败（预览模式下不可用）")
//   );
// }

const sessionBusy = () => app.busy === `session-${props.preset.name}`;

/** 该工具的图标 data URI（无图标为 undefined，前端回退圆点） */
function sdkIcon(sdkName: string): string | null | undefined {
  return app.tools.find((s) => s.name === sdkName)?.icon;
}
</script>

<template>
  <div class="proj-row">
    <div class="proj-head">
      <span class="proj-name">{{ preset.name }}</span>
      <span class="mono muted proj-dir" :title="preset.dir">{{ preset.dir }}</span>
      <div style="flex: 1" />
      <n-button size="tiny" quaternary @click="emit('edit', preset)">编辑</n-button>
      <n-button size="tiny" quaternary type="error" @click="store.deleteProjectPreset(preset.name)">
        删除
      </n-button>
    </div>

    <div class="proj-vers">
      <span v-if="preset.versions.length === 0" class="muted proj-empty">未配置版本组合</span>
      <n-tag
        v-for="v in preset.versions"
        :key="`${v.tool}-${v.distribution ?? 'default'}-${v.version}`"
        size="small"
        :bordered="false"
        class="proj-chip"
      >
        <span class="proj-chip-inner">
          <ToolIcon :icon="sdkIcon(v.tool)" :size="14" :name="v.tool" />
          {{ v.tool }} {{ v.version }}
        </span>
      </n-tag>
    </div>

    <div class="proj-actions">
      <n-button
        size="small"
        type="primary"
        :loading="sessionBusy()"
        @click="store.launchProjectSession(preset.name, 'cmd', [])"
      >
        启动终端
      </n-button>
      <!-- 启动 IDE 按钮已移除 -->
      <!-- 复制脚本按钮暂缓显示
      <n-button size="small" @click="copyScript">复制脚本</n-button>
      -->
    </div>
  </div>
</template>

<style scoped>
.proj-chip-inner {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.proj-row {
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  background: #fff;
  padding: 14px 16px;
}
.proj-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}
.proj-name {
  font-weight: 700;
  font-size: 14px;
}
.proj-dir {
  font-size: 12px;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.proj-vers {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 10px;
}
.proj-empty {
  font-size: 11px;
}
.proj-actions {
  display: flex;
  gap: 8px;
}
</style>
