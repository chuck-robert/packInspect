<script setup lang="ts">
/**
 * 应用外壳。
 *
 * 结构：
 *   自绘标题栏（TitleBar）
 *   ├─ 侧边栏（NavSidebar：上=包管理器，下=工具）
 *   └─ 主区（HeaderBar 搜索 + 提示区 + 视图）
 *   状态栏（StatusBar）
 *
 * 默认视图是 `manage`（包管理总览）；点击某个包管理器会切到 `manager`（管理器详情，含分页）。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from '@/i18n'
import TitleBar from '@/components/TitleBar.vue'
import NavSidebar from '@/components/NavSidebar.vue'
import StatusBar from '@/components/StatusBar.vue'
import GlobalBanner from '@/components/GlobalBanner.vue'
import HeaderBar from '@/components/HeaderBar.vue'
import ManagerView from '@/views/ManagerView.vue'
import ManagerDetailView from '@/views/ManagerDetailView.vue'
import CacheView from '@/views/CacheView.vue'
import RegistryView from '@/views/RegistryView.vue'
import SettingsView from '@/views/SettingsView.vue'
import ReportView from '@/views/ReportView.vue'
import CleanDialog from '@/components/CleanDialog.vue'

const store = useAppStore()
const settings = useSettingsStore()
const { t } = useI18n()

const cleanDialogOpen = ref(false)
const measureSize = ref(false)
const showReport = ref(false)

const activeTitle = computed(() => {
  switch (store.view) {
    case 'manage':
      return t('nav.manage')
    case 'settings':
      return t('nav.settings')
    case 'cache':
      return t('nav.cache')
    case 'registry':
      return t('nav.registry')
    case 'manager':
      return store.activeManagerInfo?.name ?? t('nav.managersSection')
    default:
      return t('nav.manage')
  }
})

onMounted(async () => {
  settings.initSystemTheme()
  await settings.load()
  await store.boot(settings.settings.scanOnStartup, settings.resolvedTheme)
})

/**
 * 主题变化 → 刷新包管理器 logo 配色。
 * 只重取 logo（后端有缓存），不重新探测，切换是瞬时的。
 */
watch(
  () => settings.resolvedTheme,
  async (theme) => {
    if (store.managers.length) await store.refreshLogos(theme)
  },
)

async function onScan() {
  // 在管理器详情页时只扫当前管理器，避免误以为要全量扫
  const managers = store.view === 'manager' && store.activeManager ? [store.activeManager] : []
  await store.scan({ managers, measureSize: measureSize.value })
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
      <!-- 顶部：搜索 + 操作 -->
      <HeaderBar />

      <header class="toolbar">
        <div class="toolbar__title">
          <button
            v-if="store.view === 'manager'"
            class="btn btn--ghost btn--sm"
            @click="store.setView('manage')"
          >
            ← {{ t('nav.back') }}
          </button>
          {{ activeTitle }}
          <span v-if="store.scanning || store.detecting" class="spinner" />
        </div>

        <div class="toolbar__spacer" />

        <label class="checkbox" :title="t('toolbar.measureHint')">
          <input v-model="measureSize" type="checkbox" />
          {{ t('toolbar.measure') }}
        </label>

        <button
          class="btn btn--primary"
          :disabled="store.scanning || store.installed.length === 0"
          @click="onScan"
        >
          {{ store.scanning ? t('toolbar.scanning') : t('toolbar.scan') }}
        </button>

        <button class="btn" :disabled="store.detecting" @click="store.detect(true, settings.resolvedTheme)">
          {{ store.detecting ? t('toolbar.detecting') : t('toolbar.redetect') }}
        </button>

        <button class="btn" :disabled="store.installed.length === 0" @click="openCleanDialog">
          {{ t('toolbar.clean') }}
        </button>

        <button class="btn btn--ghost" :title="t('report.title')" @click="showReport = !showReport">
          {{ showReport ? '×' : 'ⓘ' }}
        </button>
      </header>

      <GlobalBanner />

      <ManagerView v-if="store.view === 'manage'" />
      <ManagerDetailView v-else-if="store.view === 'manager'" />
      <CacheView v-else-if="store.view === 'cache'" />
      <RegistryView v-else-if="store.view === 'registry'" />
      <SettingsView v-else-if="store.view === 'settings'" />
      <ReportView v-else />

      <div v-if="showReport" class="report-overlay">
        <button class="btn btn--ghost btn--sm report-overlay__close" @click="showReport = false">✕</button>
        <ReportView />
      </div>
    </main>

    <StatusBar />

    <CleanDialog v-model:open="cleanDialogOpen" />
  </div>
</template>
