<script setup lang="ts">
/**
 * 左侧导航：视图切换 + 按语言生态分组的包管理器过滤。
 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ViewKey } from '@/types'
import { LANGUAGE_LABELS } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const views: { key: ViewKey; labelKey: string; icon: string }[] = [
  { key: 'manage', labelKey: 'nav.manage', icon: 'M3 3h7v7H3zM12 3h5v5h-5zM12 10h5v7h-5zM3 12h7v5H3z' },
  { key: 'packages', labelKey: 'nav.packages', icon: 'M3 5h14v3H3zM3 10h14v3H3zM3 15h9v3H3z' },
  { key: 'cache', labelKey: 'nav.cache', icon: 'M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16zm0 3v5l3 2' },
  { key: 'registry', labelKey: 'nav.registry', icon: 'M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM2 10h16M10 2a13 13 0 0 1 0 16 13 13 0 0 1 0-16' },
  { key: 'settings', labelKey: 'nav.settings', icon: 'M10 6.5A3.5 3.5 0 1 0 10 13.5 3.5 3.5 0 0 0 10 6.5zM10 1v3M10 16v3M1 10h3M16 10h3' },
]

const orderedGroups = computed(() =>
  [...store.groupedManagers].sort((a, b) => {
    const aHit = a.items.some((i) => i.detected) ? 0 : 1
    const bHit = b.items.some((i) => i.detected) ? 0 : 1
    return aHit - bHit || a.language.localeCompare(b.language)
  }),
)

function statusClass(detected: boolean, warnings: string[]) {
  if (!detected) return 'dot--missing'
  return warnings.length ? 'dot--warn' : 'dot--ok'
}

function selectManager(id: string | null) {
  store.setActiveManager(id)
  if (store.view !== 'packages' && store.view !== 'manage') store.setView('packages')
}
</script>

<template>
  <aside class="sidebar">
    <nav class="nav">
      <!-- 主视图 -->
      <button
        v-for="view in views"
        :key="view.key"
        class="nav__item"
        :class="{ 'is-active': store.view === view.key }"
        @click="store.setView(view.key)"
      >
        <svg class="nav__icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4">
          <path :d="view.icon" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="nav__label">{{ t(view.labelKey) }}</span>
      </button>

      <div class="nav__divider" />

      <!-- 范围过滤：只有包列表视图才需要 -->
      <div v-if="store.view === 'packages'" class="nav__group-title">{{ t('nav.all') }}</div>
      <template v-if="store.view === 'packages'">
        <button
          class="nav__item"
          :class="{ 'is-active': store.activeManager === null }"
          @click="selectManager(null)"
        >
          <span class="dot" :class="store.installed.length ? 'dot--ok' : 'dot--missing'" />
          <span class="nav__label">{{ t('nav.all') }}</span>
          <span class="nav__version">{{ store.installed.length }}</span>
        </button>

        <template v-for="group in orderedGroups" :key="group.language">
          <div class="nav__group-title">{{ LANGUAGE_LABELS[group.language] ?? group.language }}</div>
          <button
            v-for="manager in group.items"
            :key="manager.id"
            class="nav__item"
            :class="{
              'is-active': store.activeManager === manager.id,
              'is-missing': !manager.detected,
            }"
            :title="
              manager.detected
                ? `${manager.exePath ?? ''}\n${manager.globalRoot ?? ''}\n${manager.cacheDir ?? ''}`
                : t('manage.notDetectedHint')
            "
            @click="selectManager(manager.id)"
          >
            <span class="dot" :class="statusClass(manager.detected, manager.warnings)" />
            <span class="nav__label">{{ manager.name }}</span>
            <span class="nav__version">{{ manager.version ?? (manager.detected ? '?' : '—') }}</span>
          </button>
        </template>
      </template>

      <div v-if="store.booting" class="hint" style="padding: 14px 8px">{{ t('nav.loading') }}</div>
      <div v-else-if="store.detecting" class="hint" style="padding: 14px 8px">{{ t('nav.detecting') }}</div>
    </nav>
  </aside>
</template>
