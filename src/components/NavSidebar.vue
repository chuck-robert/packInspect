<script setup lang="ts">
/**
 * 左侧导航，分上下两区：
 * - 上区「包管理器」：点击进入该管理器的详情页（而不是直接跳到包列表）
 * - 下区「工具」：安装 / 缓存占用 / 镜像源 / 设置
 *
 * 用 flex 布局让下区贴底，上区独立滚动 —— 管理器最多 20 个，列表会超出一屏。
 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagerInfo, ViewKey } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'
import { shortVersion } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const tools: { key: ViewKey; labelKey: string; icon: string }[] = [
  {
    // 安装放在工具区第一位：这是最常用的"任务"，不该埋进某个管理器的分页里
    key: 'install',
    labelKey: 'nav.install',
    icon: 'M10 3v9M6 8.5l4 4 4-4M3.5 16.5h13',
  },
  {
    key: 'cache',
    labelKey: 'nav.cache',
    icon: 'M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16zm0 3v5l3 2',
  },
  {
    key: 'registry',
    labelKey: 'nav.registry',
    icon: 'M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM2 10h16M10 2a13 13 0 0 1 0 16 13 13 0 0 1 0-16',
  },
  { key: 'settings', labelKey: 'nav.settings', icon: 'M10 6.5A3.5 3.5 0 1 0 10 13.5 3.5 3.5 0 0 0 10 6.5zM10 1v3M10 16v3M1 10h3M16 10h3' },
]

const managers = computed(() => store.sidebarManagers)
const installedCount = computed(() => store.installed.length)

function isManagerActive(id: string) {
  return store.view === 'manager' && store.activeManager === id
}

/** 侧边栏只显示版本号本身；探测不到时显示占位符 */
function versionLabel(manager: ManagerInfo): string {
  if (!manager.detected) return '—'
  return shortVersion(manager.version, manager.id) || '?'
}
</script>

<template>
  <aside class="sidebar">
    <!-- 上区：包管理器 -->
    <div class="sidebar__section sidebar__section--grow">
      <div class="sidebar__heading">
        <span>{{ t('nav.managersSection') }}</span>
        <span class="sidebar__count">{{ installedCount }}/{{ managers.length }}</span>
      </div>

      <nav class="nav nav--scroll">
        <button
          v-for="manager in managers"
          :key="manager.id"
          class="nav__item"
          :class="{
            'is-active': isManagerActive(manager.id),
            'is-missing': !manager.detected,
          }"
          :title="manager.detected ? `${manager.name} ${versionLabel(manager)}` : t('nav.notInstalled')"
          @click="store.openManager(manager.id)"
        >
          <ManagerLogo :manager-id="manager.id" :name="manager.name" size="sm" />
          <span class="nav__label">{{ manager.name }}</span>
          <span class="nav__version">{{ versionLabel(manager) }}</span>
        </button>

        <div v-if="store.booting" class="hint" style="padding: 12px 8px">{{ t('nav.loading') }}</div>
        <div v-else-if="store.detecting" class="hint" style="padding: 12px 8px">
          {{ t('nav.detecting') }}
        </div>
      </nav>
    </div>

    <!-- 下区：工具 -->
    <div class="sidebar__section sidebar__section--tools">
      <div class="sidebar__heading">{{ t('nav.toolsSection') }}</div>
      <nav class="nav">
        <button
          v-for="tool in tools"
          :key="tool.key"
          class="nav__item"
          :class="{ 'is-active': store.view === tool.key }"
          @click="store.setView(tool.key)"
        >
          <svg class="nav__icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4">
            <path :d="tool.icon" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          <span class="nav__label">{{ t(tool.labelKey) }}</span>
        </button>
      </nav>
    </div>
  </aside>
</template>
