<script setup lang="ts">
// SkeletonCard —— 卡片加载骨架
import { NSkeleton } from "naive-ui";
</script>

<template>
  <div class="sk-card">
    <n-skeleton text :width="120" />
    <n-skeleton text :width="'80%'" style="margin-top: 12px" />
    <n-skeleton text :width="'100%'" style="margin-top: 8px" />
    <n-skeleton text :width="'70%'" style="margin-top: 8px" />
  </div>
</template>

<style scoped>
.sk-card {
  padding: 18px 20px;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  background: #fff;
}
</style>
