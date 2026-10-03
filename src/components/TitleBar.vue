<script setup lang="ts">
/**
 * 自绘标题栏（替代 Windows 原生标题栏）。
 *
 * 为什么自绘：原生标题栏无法跟随应用深色主题，也无法放扫描进度、当前范围这类上下文。
 * 窗口已通过 tauri.conf.json 的 `decorations: false` 去掉系统边框。
 *
 * 拖拽由 `data-tauri-drag-region` 提供（属于 core:window:allow-start-dragging 权限）。
 * 图标使用内联 SVG，不依赖任何图标库。
 */
import { computed, onMounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import { formatBytes, formatCount, formatDuration } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()
const maximized = ref(false)
const appWindow = getCurrentWindow()

const statusText = computed(() => {
  if (store.scanning) return t('toolbar.scanning')
  if (store.detecting) return t('toolbar.detecting')
  if (!store.report) return ''
  return `${formatCount(store.report.totalPackages)} ${t('toolbar.packages')} · ${formatBytes(
    store.report.totalCacheBytes,
  )} ${t('toolbar.cache')} · ${formatDuration(store.report.durationMs)}`
})

async function syncMaximized() {
  try {
    maximized.value = await appWindow.isMaximized()
  } catch {
    maximized.value = false
  }
}

onMounted(async () => {
  await syncMaximized()
  // 监听窗口尺寸变化以更新最大化按钮图标
  try {
    await appWindow.onResized(() => void syncMaximized())
  } catch {
    /* 权限或平台不支持时忽略 */
  }
})

async function minimize() {
  await appWindow.minimize()
}

async function toggleMaximize() {
  await appWindow.toggleMaximize()
  await syncMaximized()
}

async function closeWindow() {
  await appWindow.close()
}

async function onDoubleClick() {
  await toggleMaximize()
}
</script>

<template>
  <header class="titlebar" data-tauri-drag-region @dblclick="onDoubleClick">
    <div class="titlebar__brand" data-tauri-drag-region>
      <span class="titlebar__mark">PI</span>
      <span class="titlebar__name">{{ t('app.name') }}</span>
      <span class="titlebar__tag">{{ t('app.tagline') }}</span>
    </div>

    <div class="titlebar__center" data-tauri-drag-region>
      <span v-if="store.scanning || store.detecting" class="spinner" />
      <span class="titlebar__status mono">{{ statusText }}</span>
    </div>

    <div class="titlebar__controls">
      <button class="winbtn" :title="t('titlebar.minimize')" @click="minimize">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1.1" /></svg>
      </button>
      <button
        class="winbtn"
        :title="maximized ? t('titlebar.restore') : t('titlebar.maximize')"
        @click="toggleMaximize"
      >
        <svg v-if="!maximized" width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.1" />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1.1" />
          <path d="M2.5 2.5V0.5h7v7h-2" fill="none" stroke="currentColor" stroke-width="1.1" />
        </svg>
      </button>
      <button class="winbtn winbtn--close" :title="t('titlebar.close')" @click="closeWindow">
        <svg width="10" height="10" viewBox="0 0 10 10">
          <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" stroke-width="1.1" />
        </svg>
      </button>
    </div>
  </header>
</template>
