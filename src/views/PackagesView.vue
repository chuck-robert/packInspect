<script setup lang="ts">
/**
 * 已安装包列表。
 *
 * 按要求只展示「包名 + 版本」两列（体积可选），路径等细节收进详情抽屉与右键菜单，
 * 让主列表保持扫读友好。
 *
 * 交互：
 * - 左键行 → 打开详情抽屉
 * - 右键行 → 打开管理菜单（含更新 / 卸载 / 安装占位按钮 + 包内插件入口）
 * - 右键空白 → 无操作（避免误触）
 */
import { computed, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from '@/i18n'
import type { PackageRecord } from '@/types'
import PackageContextMenu from '@/components/PackageContextMenu.vue'
import PackageDetailDrawer from '@/components/PackageDetailDrawer.vue'
import { formatBytesShort, formatCount } from '@/utils/format'

const store = useAppStore()
const settings = useSettingsStore()
const { t } = useI18n()

type SortKey = 'name' | 'version' | 'size'

const sortKey = ref<SortKey>('name')
const sortAsc = ref(true)
const detailTarget = ref<PackageRecord | null>(null)
const menuState = ref<{ record: PackageRecord; x: number; y: number } | null>(null)

const rows = computed(() => {
  const list = [...store.visiblePackages]
  const dir = sortAsc.value ? 1 : -1
  list.sort((a, b) => {
    switch (sortKey.value) {
      case 'size':
        return ((a.size ?? 0) - (b.size ?? 0)) * dir
      case 'version':
        return (a.version ?? '').localeCompare(b.version ?? '', undefined, { numeric: true }) * dir
      default:
        return a.name.localeCompare(b.name) * dir
    }
  })
  return list
})

const redundantCount = computed(() => store.visiblePackages.filter((p) => p.redundant).length)

function toggleSort(key: SortKey) {
  if (sortKey.value === key) sortAsc.value = !sortAsc.value
  else {
    sortKey.value = key
    sortAsc.value = true
  }
}

function sortIndicator(key: SortKey) {
  if (sortKey.value !== key) return ''
  return sortAsc.value ? ' ▲' : ' ▼'
}

function openMenu(record: PackageRecord, event: MouseEvent) {
  menuState.value = { record, x: event.clientX, y: event.clientY }
}

function openDetail(record: PackageRecord) {
  detailTarget.value = record
}
</script>

<template>
  <section class="scroll-area" style="padding: 12px 14px">
    <!-- 过滤栏 -->
    <div class="row" style="margin-bottom: 10px">
      <input
        v-model="store.keyword"
        class="input input--search"
        type="search"
        :placeholder="t('packages.search')"
      />
      <label class="checkbox">
        <input v-model="store.onlyRedundant" type="checkbox" />
        {{ t('packages.onlyRedundant') }}
      </label>
      <span v-if="redundantCount" class="tag tag--warn">
        {{ t('packages.redundant') }} {{ redundantCount }}
      </span>
      <span class="tag">{{ t('packages.count', { count: formatCount(rows.length) }) }}</span>
      <span class="banner__spacer" />
      <span v-if="store.measuredBytes > 0" class="hint">
        {{ formatBytesShort(store.measuredBytes) }}
      </span>
    </div>

    <!-- 空态 -->
    <div v-if="!store.report" class="empty">
      <div class="empty__icon">📦</div>
      <div>{{ t('packages.emptyTitle') }}</div>
      <div class="hint">{{ t('packages.emptyHint') }}</div>
    </div>

    <div v-else-if="rows.length === 0" class="empty">
      <div class="empty__icon">🔍</div>
      <div>{{ t('packages.noMatch') }}</div>
      <div class="hint">{{ t('packages.noMatchHint') }}</div>
    </div>

    <!-- 列表 -->
    <div v-else class="panel">
      <div class="table-wrap" style="max-height: calc(100vh - 250px)">
        <table class="data pkg-table">
          <thead>
            <tr>
              <th class="is-sortable" @click="toggleSort('name')">
                {{ t('packages.name') }} / {{ t('packages.version') }}{{ sortIndicator('name') }}
              </th>
              <th
                v-if="store.measuredBytes > 0"
                class="is-sortable num"
                style="width: 96px"
                @click="toggleSort('size')"
              >
                {{ t('packages.size') }}{{ sortIndicator('size') }}
              </th>
              <th style="width: 84px" />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="p in rows"
              :key="`${p.manager}:${p.name}:${p.version}`"
              :class="{ 'is-redundant': p.redundant }"
              @contextmenu.prevent="openMenu(p, $event)"
            >
              <!-- 只展示「包名 + 版本」：版本紧跟包名，避免被挤出可视区 -->
              <td>
                <div class="pkg-cell">
                  <img
                    v-if="settings.settings.showIcons && p.icon"
                    class="pkg-icon"
                    :src="p.icon"
                    alt=""
                    loading="lazy"
                  />
                  <span v-else class="pkg-icon pkg-icon--fallback">
                    {{ p.name.replace(/^@[^/]+\//, '').charAt(0).toUpperCase() }}
                  </span>
                  <span class="pkg-cell__name" :title="p.description ?? p.name">{{ p.name }}</span>
                  <span class="pkg-cell__version mono">{{ p.version ?? '—' }}</span>
                  <span v-if="p.redundant" class="tag tag--warn">{{ t('packages.redundantMark') }}</span>
                </div>
              </td>
              <td v-if="store.measuredBytes > 0" class="num">
                {{ p.size === null ? '—' : formatBytesShort(p.size) }}
              </td>
              <td>
                <button class="btn btn--ghost btn--sm" @click="openDetail(p)">
                  {{ t('detail.title') }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <p class="hint" style="margin-top: 8px">
      提示：在包上<strong>右键</strong>可以打开管理菜单（更新 / 卸载 / 安装为占位按钮，会给出等价官方命令）。
    </p>

    <PackageContextMenu
      v-if="menuState"
      :record="menuState.record"
      :x="menuState.x"
      :y="menuState.y"
      @close="menuState = null"
      @inspect="openDetail"
      @manage="openDetail"
    />

    <PackageDetailDrawer :record="detailTarget" @close="detailTarget = null" />
  </section>
</template>
