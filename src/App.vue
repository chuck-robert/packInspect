<script setup lang="ts">
/**
 * 应用外壳：侧边栏导航 + 主区视图切换 + 状态栏 + 全局提示。
 * 视图状态（packages / cache / registry / report）是纯 UI 状态，放在组件本地，
 * 跨视图共享的数据一律走 store。
 */
import { computed, onMounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import NavSidebar from '@/components/NavSidebar.vue'
import StatusBar from '@/components/StatusBar.vue'
import GlobalBanner from '@/components/GlobalBanner.vue'
import PackagesView from '@/views/PackagesView.vue'
import CacheView from '@/views/CacheView.vue'
import RegistryView from '@/views/RegistryView.vue'
import ReportView from '@/views/ReportView.vue'
import CleanDialog from '@/components/CleanDialog.vue'
import { formatBytes, formatCount, formatDuration } from '@/utils/format'

type ViewKey = 'packages' | 'cache' | 'registry' | 'report'

const store = useAppStore()
const view = ref<ViewKey>('packages')
const cleanDialogOpen = ref(false)
const measureSize = ref(false)

const VIEWS: { key: ViewKey; label: string }[] = [
  { key: 'packages', label: '已安装包' },
  { key: 'cache', label: '缓存占用' },
  { key: 'registry', label: '镜像源' },
  { key: 'report', label: '报告与日志' },
]

const activeTitle = computed(() => {
  const name = store.activeManager
    ? store.managers.find((m) => m.id === store.activeManager)?.name ?? store.activeManager
    : '全部包管理器'
  return name
})

onMounted(() => {
  void store.boot()
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
    <NavSidebar />

    <main class="main">
      <!-- 工具栏 -->
      <header class="toolbar">
        <div class="toolbar__title">
          {{ activeTitle }}
          <span v-if="store.scanning" class="spinner" />
        </div>

        <div class="toolbar__stats">
          <span v-if="store.report">
            包 {{ formatCount(store.report.totalPackages) }}
          </span>
          <span v-if="store.report">
            缓存 {{ formatBytes(store.report.totalCacheBytes) }}
          </span>
          <span v-if="store.report">
            耗时 {{ formatDuration(store.report.durationMs) }}
          </span>
        </div>

        <div class="toolbar__spacer" />

        <label class="checkbox" title="统计每个包目录的体积（较慢，需要遍历文件系统）">
          <input v-model="measureSize" type="checkbox" />
          统计包体积
        </label>

        <button
          class="btn btn--primary"
          :disabled="store.scanning || store.installed.length === 0"
          @click="onScan"
        >
          {{ store.scanning ? '扫描中…' : '开始扫描' }}
        </button>

        <button class="btn" :disabled="store.scanning" @click="store.detect(true)">
          {{ store.detecting ? '探测中…' : '重新探测' }}
        </button>

        <button class="btn" :disabled="store.installed.length === 0" @click="openCleanDialog">
          清理缓存
        </button>
      </header>

      <!-- 全局提示区 -->
      <GlobalBanner />

      <!-- 视图切换 -->
      <nav class="row" style="padding: 8px 14px 0; gap: 4px">
        <button
          v-for="v in VIEWS"
          :key="v.key"
          class="btn btn--ghost btn--sm"
          :class="{ 'btn--primary': view === v.key }"
          @click="view = v.key"
        >
          {{ v.label }}
        </button>
      </nav>

      <PackagesView v-if="view === 'packages'" />
      <CacheView v-else-if="view === 'cache'" />
      <RegistryView v-else-if="view === 'registry'" />
      <ReportView v-else />
    </main>

    <StatusBar />

    <CleanDialog v-model:open="cleanDialogOpen" />
  </div>
</template>
