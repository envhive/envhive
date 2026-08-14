<script setup lang="ts">
// EnvTable —— 环境变量表格（等宽字体值、可复制）
import { NButton } from "naive-ui";
import type { EnvVarRow } from "../types";

const props = defineProps<{ rows: EnvVarRow[] }>();

function copyRow(row: EnvVarRow) {
  void navigator.clipboard?.writeText(`${row.key}=${row.value}`).catch(() => {});
}
</script>

<template>
  <div class="env-table">
    <n-table :bordered="false" size="small" single-line>
      <thead>
        <tr>
          <th style="width: 22%">变量名</th>
          <th style="width: 44%">值</th>
          <th style="width: 20%">来源</th>
          <th style="width: 60px" />
        </tr>
      </thead>
      <tbody>
        <tr v-for="(row, i) in props.rows" :key="i">
          <td><span class="env-key-text">{{ row.key }}</span></td>
          <td><span class="env-val-text">{{ row.value }}</span></td>
          <td><span class="mono">{{ row.source }}</span></td>
          <td>
            <n-button size="tiny" quaternary @click="copyRow(row)">复制</n-button>
          </td>
        </tr>
      </tbody>
    </n-table>
  </div>
</template>

<style scoped>
.env-table {
  margin-top: 8px;
}
.env-key-text {
  font-weight: 600;
  font-size: 12.5px;
}
.env-val-text {
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
  font-size: 12px;
  word-break: break-all;
  color: #1f2329;
}
</style>
