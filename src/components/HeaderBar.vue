<script setup lang="ts">
/**
 * 顶部搜索 + 操作区。
 *
 * 搜索会同时命中「包管理器」与「已安装包」：
 * - 命中包管理器 → 直接进入该管理器的详情页
 * - 命中已安装包 → 进入其管理器的「包列表」分页并定位到该包
 *
 * 结果里区分两类（分组标题 + 图标），避免同名包与管理器混淆。
 */
import { computed, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { SearchHit } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'

const store = useAppStore()
const { t } = useI18n()

const focused = ref(false)
const activeIndex = ref(0)

const hits = computed(() => store.searchHits)
const managerHits = computed(() => hits.value.filter((h) => h.kind === 'manager'))
const packageHits = computed(() => hits.value.filter((h) => h.kind === 'package'))
const showPanel = computed(() => focused.value && store.keyword.trim().length > 0)
const truncated = computed(() => hits.value.length >= 50)

// 输入变化时重置高亮
watch(
  () => store.keyword,
  () => {
    activeIndex.value = 0
  },
)

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

function indexOfGroup(list: SearchHit[], hit: SearchHit) {
  return list.indexOf(hit)
}
</script>

<template>
  <header class="toolbar header-bar">
    <!-- 搜索框 -->
    <div class="searchbox" :class="{ 'is-focused': focused }">
      <svg class="searchbox__icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6">
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
      <div v-if="showPanel" class="search-panel" @mousedown.prevent>
        <template v-if="hits.length === 0">
          <div class="search-panel__empty">{{ t('search.empty') }}</div>
        </template>

        <template v-else>
          <template v-if="managerHits.length">
            <div class="search-panel__group">{{ t('search.managers') }}</div>
            <button
              v-for="hit in managerHits"
              :key="`m-${hit.managerId}`"
              class="search-hit"
              :class="{ 'is-active': activeIndex === indexOfGroup(hits, hit) }"
              @mouseenter="activeIndex = indexOfGroup(hits, hit)"
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
              :key="`p-${hit.managerId}-${hit.label}`"
              class="search-hit"
              :class="{ 'is-active': activeIndex === indexOfGroup(hits, hit) }"
              @mouseenter="activeIndex = indexOfGroup(hits, hit)"
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

    <div class="toolbar__spacer" />

    <div class="toolbar__stats">
      <span v-if="store.report">{{ store.report.totalPackages }} {{ t('toolbar.packages') }}</span>
      <span v-if="store.report">{{ (store.report.totalCacheBytes / 1024 / 1024 / 1024).toFixed(2) }} GB</span>
    </div>
  </header>
</template>
