<script setup lang="ts">
/**
 * 底部状态栏。
 *
 * 除了主机/范围信息，还承担**扫描进度展示**：
 * 渐进扫描时显示「已完成 / 总数」进度条与当前正在扫描的管理器 ——
 * 这是用户判断「是不是卡住了」的主要依据，因此放在常驻可见的位置。
 * 失败的管理器也会在这里明确列出，而不是静默吞掉。
 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import { formatBytes, formatCount, formatDate, formatDuration } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const progress = computed(() => store.scanProgress)
const percent = computed(() =>
  progress.value.total === 0
    ? 0
    : Math.round((progress.value.completed / progress.value.total) * 100),
)
const failedIds = computed(() => Object.keys(progress.value.failed))
const currentName = computed(() => {
  const id = progress.value.current
  if (!id) return ''
  return store.managers.find((m) => m.id === id)?.name ?? id
})

const scanState = computed(() => {
  if (store.scanning) return t('toolbar.scanning')
  if (store.detecting) return t('toolbar.detecting')
  if (!store.report) return '—'
  return formatDate(store.report.generatedAt)
})

const host = computed(() => {
  const report = store.report
  if (!report) return '—'
  return `${report.hostname ?? 'unknown'} · ${report.os}`
})
</script>

<template>
  <footer class="statusbar">
    <!-- 扫描进度：优先展示，占满中部空间 -->
    <div v-if="store.scanning" class="statusbar__progress">
      <div class="statusbar__bar">
        <div class="statusbar__bar-fill" :style="{ width: `${percent}%` }" />
      </div>
      <span class="statusbar__text mono">
        {{ t('status.scanningProgress', { done: progress.completed, total: progress.total }) }}
        <template v-if="currentName"> · {{ currentName }}</template>
      </span>
    </div>

    <template v-else>
      <span>{{ host }}</span>
      <span>{{ scanState }}</span>
      <span v-if="store.report">
        {{ formatCount(store.report.totalPackages) }} {{ t('toolbar.packages') }} /
        {{ formatBytes(store.report.totalCacheBytes) }}
      </span>
      <span v-if="store.report">{{ formatDuration(store.report.durationMs) }}</span>
    </template>

    <span style="flex: 1" />

    <!-- 失败的管理器：明确列出而不是静默吞掉 -->
    <span v-if="failedIds.length" class="statusbar__failed" :title="t('status.scanFailedHint')">
      {{ t('status.scanFailed', { count: failedIds.length }) }}
    </span>

    <span v-if="store.keyword" class="statusbar__filter">
      {{ t('status.filteredBy', { keyword: store.keyword }) }}
    </span>

    <span>{{ store.activeManager ?? t('nav.all') }}</span>
  </footer>
</template>
