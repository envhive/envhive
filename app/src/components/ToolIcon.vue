<script setup lang="ts">
// ToolIcon ——工具图标统一渲染：有图标显示 <img>（data URI），
// 加载失败 / 无图标回退 DOT_COLORS 彩色圆点（保持旧版身份标识语义）
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { DOT_COLORS } from "../types";

const props = withDefaults(
  defineProps<{
    /**工具图标 data URI（null/空串 = 无图标） */
    icon?: string | null;
    /** 图标渲染尺寸（px） */
    size?: number;
    /** 无图标时圆点取色的索引（按工具列表顺序） */
    index?: number;
    /** 无障碍 alt 文本 */
    name?: string;
  }>(),
  { icon: null, size: 18, index: 0, name: "" }
);

const { t } = useI18n();
const failed = ref(false);
const showImg = computed(() => !!props.icon && !failed.value);
const imgSrc = computed(() => props.icon ?? undefined);
const fallbackColor = computed(() => DOT_COLORS[props.index % DOT_COLORS.length]);
</script>

<template>
  <span
    class="tool-icon"
    :style="{ width: `${size}px`, height: `${size}px` }"
    :title="name"
  >
    <img
      v-if="showImg"
      :src="imgSrc"
      :alt="name || t('common.toolIconAlt')"
      class="tool-icon-img"
      :style="{ width: `${size}px`, height: `${size}px` }"
      draggable="false"
      @error="failed = true"
    />
    <span v-else class="tool-icon-dot" :style="{ background: fallbackColor }" />
  </span>
</template>

<style scoped>
.tool-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.tool-icon-img {
  object-fit: contain;
  border-radius: 4px;
  display: block;
}
.tool-icon-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  display: block;
}
</style>
