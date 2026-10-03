<script setup lang="ts">
/**
 * 底部状态栏。
 *
 * 左侧是常驻信息（主机、扫描时间、包数、缓存），
 * **右侧**是扫描进度（进度条 + 已完成/总数 + 当前管理器）。
 *
 * 进度条放在这一行的右端而不是左端：
 * 左端紧邻侧边栏，阅读上会被当成「侧边栏内容的一部分」；
 * 而且左端要与主机名、时间等文字挤在一起，扫描开始/结束时文字长度跳变明显。
 * 放在右端（`flex: 1` 撑开中间空隙之后）位置稳定，也不干扰左侧信息的读取。
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
const currentName = computed(() => {
  const id = progress.value.current
  if (!id) return ''
  return store.managers.find((m) => m.id === id)?.name ?? id
})
const failedIds = computed(() => Object.keys(progress.value.failed))

const scanState = computed(() => {
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
    <!-- 左：常驻信息 -->
    <span>{{ host }}</span>
    <span>{{ scanState }}</span>
    <span v-if="store.report">
      {{ formatCount(store.report.totalPackages) }} {{ t('toolbar.packages') }} /
      {{ formatBytes(store.report.totalCacheBytes) }}
    </span>
    <span v-if="store.report && !store.scanning">{{ formatDuration(store.report.durationMs) }}</span>

    <!-- 中间空隙：把进度推到右端 -->
    <span style="flex: 1" />

    <!-- 失败项提示 -->
    <span v-if="failedIds.length" class="statusbar__failed" :title="t('status.scanFailedHint')">
      {{ t('status.scanFailed', { count: failedIds.length }) }}
    </span>

    <span v-if="store.keyword" class="statusbar__filter">
      {{ t('status.filteredBy', { keyword: store.keyword }) }}
    </span>

    <!-- 右：扫描进度（footer 内，不是独立浮层） -->
    <div v-if="store.scanning" class="statusbar__progress">
      <span class="mono statusbar__progress-text">
        {{ t('toolbar.scanning') }} {{ progress.completed }}/{{ progress.total }}
        <template v-if="currentName"> · {{ currentName }}</template>
      </span>
      <div class="statusbar__bar">
        <div class="statusbar__bar-fill" :style="{ width: `${percent}%` }" />
      </div>
    </div>

    <span v-if="!store.scanning">{{ store.activeManager ?? t('nav.all') }}</span>
  </footer>
</template>
