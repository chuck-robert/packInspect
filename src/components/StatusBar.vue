<script setup lang="ts">
/** 底部状态栏：主机信息、探测/扫描状态、当前范围统计 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import { formatBytes, formatCount, formatDateTime } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const scanState = computed(() => {
  if (store.scanning) return t('toolbar.scanning')
  if (store.detecting) return t('toolbar.detecting')
  if (!store.report) return '—'
  return `${t('report.generatedAt')} ${formatDateTime(store.report.generatedAt)}`
})

const host = computed(() => {
  const report = store.report
  if (!report) return '—'
  return `${report.hostname ?? 'unknown'} · ${report.os}`
})
</script>

<template>
  <footer class="statusbar">
    <span>{{ host }}</span>
    <span>{{ scanState }}</span>
    <span v-if="store.report">
      {{ formatCount(store.report.totalPackages) }} {{ t('toolbar.packages') }} /
      {{ formatBytes(store.report.totalCacheBytes) }}
    </span>
    <span style="flex: 1" />
    <span>{{ store.activeManager ?? t('nav.all') }}</span>
  </footer>
</template>
