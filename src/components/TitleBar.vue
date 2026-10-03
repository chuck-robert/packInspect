<script setup lang="ts">
/**
 * 自绘标题栏。
 *
 * 布局：左侧品牌 · 中间搜索框 · 右侧窗口控制
 * - 窗口通过 tauri.conf.json 的 `decorations: false` 去掉系统边框
 * - 拖拽由 `data-tauri-drag-region` 提供（core:window:allow-start-dragging 权限）
 * - 搜索框放在这里而不是工具栏：顶部空间更宽敞，且它是全局入口
 *
 * 下拉结果的键盘导航必须配合 scrollIntoView —— 结果多时高亮项会跑到可视区外，
 * 用户看不到自己选中了什么。
 */
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { SearchHit } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'

const store = useAppStore()
const { t } = useI18n()

const maximized = ref(false)
const appWindow = getCurrentWindow()

const focused = ref(false)
const activeIndex = ref(0)
/** 结果面板容器与条目，用于把高亮项滚进可视区 */
const panelRef = ref<HTMLElement | null>(null)
const itemRefs = ref<Record<number, HTMLElement | null>>({})

const hits = computed(() => store.searchHits)
const managerHits = computed(() => hits.value.filter((h) => h.kind === 'manager'))
const packageHits = computed(() => hits.value.filter((h) => h.kind === 'package'))
const showPanel = computed(() => focused.value && store.keyword.trim().length > 0)
const truncated = computed(() => hits.value.length >= 50)

async function syncMaximized() {
  try {
    maximized.value = await appWindow.isMaximized()
  } catch {
    maximized.value = false
  }
}

onMounted(async () => {
  await syncMaximized()
  try {
    await appWindow.onResized(() => void syncMaximized())
  } catch {
    /* 平台不支持时忽略 */
  }
})

function pick(hit: SearchHit) {
  if (hit.kind === 'manager') {
    store.openManager(hit.managerId, 'overview')
  } else {
    // 包命中：进入该管理器的包列表分页，并保留关键字作为过滤条件
    const record = hit.record
    store.openManager(hit.managerId, 'packages')
    store.keyword = record?.name ?? hit.label
  }
  focused.value = false
}

/** 把当前高亮项滚进可视区（元素还没渲染时跳过） */
async function scrollActiveIntoView() {
  await nextTick()
  itemRefs.value[activeIndex.value]?.scrollIntoView({ block: 'nearest' })
}

watch(activeIndex, () => void scrollActiveIntoView())

// 关键字变化后结果集重建：重置高亮并滚回顶部
watch(
  () => store.keyword,
  async () => {
    activeIndex.value = 0
    await nextTick()
    panelRef.value?.scrollTo({ top: 0 })
  },
)

function onKeydown(event: KeyboardEvent) {
  if (!showPanel.value) return
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    activeIndex.value = Math.min(activeIndex.value + 1, hits.value.length - 1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    activeIndex.value = Math.max(activeIndex.value - 1, 0)
  } else if (event.key === 'Enter') {
    event.preventDefault()
    const target = hits.value[activeIndex.value]
    if (target) pick(target)
  } else if (event.key === 'Escape') {
    focused.value = false
  }
}

function setItemRef(hit: SearchHit, el: Element | null) {
  const index = hits.value.indexOf(hit)
  if (index >= 0) itemRefs.value[index] = el as HTMLElement | null
}

function isActive(hit: SearchHit) {
  return activeIndex.value === hits.value.indexOf(hit)
}

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
</script>

<template>
  <header class="titlebar">
    <div class="titlebar__brand" data-tauri-drag-region>
      <span class="titlebar__mark">PI</span>
      <span class="titlebar__name">{{ t('app.name') }}</span>
    </div>

    <!-- 搜索框：全局入口，位于标题栏中部 -->
    <div class="titlebar__search">
      <div class="searchbox" :class="{ 'is-focused': focused }">
        <svg
          class="searchbox__icon"
          viewBox="0 0 20 20"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
        >
          <circle cx="9" cy="9" r="5.5" />
          <path d="M13 13l4 4" stroke-linecap="round" />
        </svg>
        <input
          v-model="store.keyword"
          class="searchbox__input"
          type="text"
          :placeholder="t('search.placeholder')"
          @focus="focused = true"
          @blur="focused = false"
          @keydown="onKeydown"
        />
        <button
          v-if="store.keyword"
          class="searchbox__clear"
          @mousedown.prevent
          @click="store.keyword = ''"
        >
          ✕
        </button>

        <!-- 结果面板 -->
        <div v-if="showPanel" ref="panelRef" class="search-panel" @mousedown.prevent>
          <div v-if="hits.length === 0" class="search-panel__empty">{{ t('search.empty') }}</div>

          <template v-else>
            <template v-if="managerHits.length">
              <div class="search-panel__group">{{ t('search.managers') }}</div>
              <button
                v-for="hit in managerHits"
                :key="`m-${hit.managerId}`"
                :ref="(el) => setItemRef(hit, el as Element | null)"
                class="search-hit"
                :class="{ 'is-active': isActive(hit) }"
                @mouseenter="activeIndex = hits.indexOf(hit)"
                @click="pick(hit)"
              >
                <ManagerLogo :manager-id="hit.managerId" :name="hit.label" size="sm" />
                <span class="search-hit__label">{{ hit.label }}</span>
                <span class="search-hit__detail mono">{{ hit.detail }}</span>
              </button>
            </template>

            <template v-if="packageHits.length">
              <div class="search-panel__group">{{ t('search.packages') }}</div>
              <button
                v-for="hit in packageHits"
                :key="`p-${hit.managerId}-${hit.label}-${hit.record?.version ?? ''}`"
                :ref="(el) => setItemRef(hit, el as Element | null)"
                class="search-hit"
                :class="{ 'is-active': isActive(hit) }"
                @mouseenter="activeIndex = hits.indexOf(hit)"
                @click="pick(hit)"
              >
                <ManagerLogo :manager-id="hit.managerId" :name="hit.managerName" size="sm" />
                <span class="search-hit__label">{{ hit.label }}</span>
                <span class="search-hit__detail mono">{{ hit.detail }}</span>
              </button>
            </template>

            <div v-if="truncated" class="search-panel__foot">{{ t('search.more') }}</div>
          </template>
        </div>
      </div>
    </div>

    <!-- 拖拽填充区：保证标题栏空白处也能拖动窗口 -->
    <div class="titlebar__spacer" data-tauri-drag-region />

    <div class="titlebar__controls">
      <button class="winbtn" :title="t('titlebar.minimize')" @click="minimize">
        <svg width="10" height="10" viewBox="0 0 10 10">
          <path d="M0 5h10" stroke="currentColor" stroke-width="1.1" />
        </svg>
      </button>
      <button
        class="winbtn"
        :title="maximized ? t('titlebar.restore') : t('titlebar.maximize')"
        @click="toggleMaximize"
      >
        <svg v-if="!maximized" width="10" height="10" viewBox="0 0 10 10">
          <rect
            x="0.5"
            y="0.5"
            width="9"
            height="9"
            fill="none"
            stroke="currentColor"
            stroke-width="1.1"
          />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10">
          <rect
            x="0.5"
            y="2.5"
            width="7"
            height="7"
            fill="none"
            stroke="currentColor"
            stroke-width="1.1"
          />
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
