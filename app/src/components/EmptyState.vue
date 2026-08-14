<script setup lang="ts">
// EmptyState —— 空状态：说明 + 引导按钮
import { NEmpty, NButton } from "naive-ui";

defineProps<{
  hint?: string;
  actionLabel?: string;
}>();

const emit = defineEmits<{ action: [] }>();
</script>

<template>
  <div class="es">
    <n-empty :description="hint ?? ''">
      <template #extra>
        <n-button v-if="actionLabel" type="primary" size="small" @click="emit('action')">
          {{ actionLabel }}
        </n-button>
      </template>
    </n-empty>
  </div>
</template>

<style scoped>
.es {
  padding: 30px 0;
  display: flex;
  justify-content: center;
}
</style>
