<script setup lang="ts">
// StatusChip —— 状态徽标（包装 Naive UI n-tag）
// 已安装=绿、当前使用=紫、有新版本=橙、冲突=红、未安装=灰
import { computed } from "vue";
import { NTag } from "naive-ui";
import type { TagProps } from "naive-ui";

export type ChipTone = "ok" | "accent" | "warn" | "danger" | "muted" | "info";

const props = defineProps<{
  tone: ChipTone;
  title?: string;
}>();

const type = computed<TagProps["type"]>(() => {
  switch (props.tone) {
    case "ok":
      return "success";
    case "accent":
      return "primary";
    case "warn":
      return "warning";
    case "danger":
      return "error";
    case "muted":
      return "default";
    case "info":
      return "info";
    default:
      return "default";
  }
});
</script>

<template>
  <n-tag :type="type" size="small" round :bordered="false" :title="title">
    <slot />
  </n-tag>
</template>
