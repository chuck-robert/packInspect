<script setup lang="ts">
/**
 * 应用外壳。
 *
 * 结构：
 *   自绘标题栏（TitleBar）
 *   ├─ 侧边栏（NavSidebar）
 *   └─ 主区（工具栏 + 提示区 + 视图）
 *   状态栏（StatusBar）
 *
 * 默认视图是 `manage`（包管理页），满足「打开的页面默认是管理页面」。
 */
import { computed, onMounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from '@/i18n'
import TitleBar from '@/components/TitleBar.vue'
import NavSidebar from '@/components/NavSidebar.vue'
import StatusBar from '@/components/StatusBar.vue'
import GlobalBanner from '@/components/GlobalBanner.vue'
import ManagerView from '@/views/ManagerView.vue'
import PackagesView from '@/views/PackagesView.vue'
import CacheView from '@/views/CacheView.vue'
import RegistryView from '@/views/RegistryView.vue'
import SettingsView from '@/views/SettingsView.vue'
import ReportView from '@/views/ReportView.vue'
import CleanDialog from '@/components/CleanDialog.vue'
import { formatBytes, formatCount, formatDuration } from '@/utils/format'

const store = useAppStore()
const settings = useSettingsStore()
const { t } = useI18n()

const cleanDialogOpen = ref(false)
const measureSize = ref(false)
const showReport = ref(false)

const activeTitle = computed(() => {
  if (store.view === 'manage') return t('nav.manage')
  if (store.view === 'settings') return t('nav.settings')
  if (store.view === 'cache') return t('nav.cache')
  if (store.view === 'registry') return t('nav.registry')
  const name = store.activeManager
    ? store.managers.find((m) => m.id === store.activeManager)?.name ?? store.activeManager
    : t('nav.all')
  return `${t('nav.packages')} · ${name}`
})

/** 扫描按钮的语义：有选中管理器就只扫它，否则全量扫 */
const scanScopeHint = computed(() =>
  store.activeManager ? store.activeManager : t('nav.all'),
)

onMounted(async () => {
  settings.initSystemTheme()
  await settings.load()
  await store.boot(settings.settings.scanOnStartup)
})

async function onScan() {
  await store.scan({ measureSize: measureSize.value })
}

async function openCleanDialog() {
  cleanDialogOpen.value = true
  if (store.candidates.length === 0) {
    await store.loadCandidates()
  }
}
</script>

<template>
  <div class="app-shell">
    <TitleBar />

    <NavSidebar />

    <main class="main">
      <!-- 工具栏 -->
      <header class="toolbar">
        <div class="toolbar__title">
          {{ activeTitle }}
          <span v-if="store.scanning || store.detecting" class="spinner" />
        </div>

        <div class="toolbar__stats">
          <span v-if="store.report">{{ formatCount(store.report.totalPackages) }} {{ t('toolbar.packages') }}</span>
          <span v-if="store.report">{{ formatBytes(store.report.totalCacheBytes) }} {{ t('toolbar.cache') }}</span>
          <span v-if="store.report">{{ formatDuration(store.report.durationMs) }}</span>
        </div>

        <div class="toolbar__spacer" />

        <label class="checkbox" :title="t('toolbar.measureHint')">
          <input v-model="measureSize" type="checkbox" />
          {{ t('toolbar.measure') }}
        </label>

        <button
          class="btn btn--primary"
          :disabled="store.scanning || store.installed.length === 0"
          :title="scanScopeHint"
          @click="onScan"
        >
          {{ store.scanning ? t('toolbar.scanning') : t('toolbar.scan') }}
        </button>

        <button class="btn" :disabled="store.detecting" @click="store.detect(true)">
          {{ store.detecting ? t('toolbar.detecting') : t('toolbar.redetect') }}
        </button>

        <button class="btn" :disabled="store.installed.length === 0" @click="openCleanDialog">
          {{ t('toolbar.clean') }}
        </button>

        <button class="btn btn--ghost" :title="t('report.title')" @click="showReport = !showReport">
          {{ showReport ? '×' : 'ⓘ' }}
        </button>
      </header>

      <!-- 全局提示区 -->
      <GlobalBanner />

      <!-- 视图 -->
      <ManagerView v-if="store.view === 'manage'" />
      <PackagesView v-else-if="store.view === 'packages'" />
      <CacheView v-else-if="store.view === 'cache'" />
      <RegistryView v-else-if="store.view === 'registry'" />
      <SettingsView v-else-if="store.view === 'settings'" />
      <ReportView v-else />

      <!-- 报告面板（从工具栏 ⓘ 打开，复用右下的浮层） -->
      <div v-if="showReport" class="report-overlay">
        <button class="btn btn--ghost btn--sm report-overlay__close" @click="showReport = false">✕</button>
        <ReportView />
      </div>
    </main>

    <StatusBar />

    <CleanDialog v-model:open="cleanDialogOpen" />
  </div>
</template>
