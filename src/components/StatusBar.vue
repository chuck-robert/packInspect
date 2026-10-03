<script setup lang="ts">
/** 底部状态栏：主机信息、探测/扫描状态、当前视图统计 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { formatBytes, formatCount, formatDateTime } from '@/utils/format'

const store = useAppStore()

const scanState = computed(() => {
  if (store.scanning) return '扫描中…'
  if (store.detecting) return '探测中…'
  if (!store.report) return '待扫描'
  return `报告时间 ${formatDateTime(store.report.generatedAt)}`
})

const host = computed(() => {
  const r = store.report
  if (!r) return '—'
  return `${r.hostname ?? 'unknown'} · ${r.os}`
})
</script>

<template>
  <footer class="statusbar">
    <span>{{ host }}</span>
    <span>{{ scanState }}</span>
    <span v-if="store.report">
      {{ formatCount(store.report.totalPackages) }} 包 / {{ formatBytes(store.report.totalCacheBytes) }} 缓存
    </span>
    <span style="flex: 1" />
    <span v-if="store.activeManager">范围：{{ store.activeManager }}</span>
    <span v-else>范围：全部</span>
  </footer>
</template>
