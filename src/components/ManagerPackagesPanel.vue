<script setup lang="ts">
/**
 * 某个包管理器的已安装包列表（管理器详情页的「包列表」分页）。
 *
 * 只展示「包名 + 版本」—— 路径等细节收进右键菜单与详情抽屉，保持列表可扫读。
 * 右键行可打开管理菜单（含更新 / 卸载 / 安装占位按钮与包内插件入口）。
 */
import { computed, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagementAction, ManagerInfo, PackageRecord } from '@/types'
import PackageContextMenu from '@/components/PackageContextMenu.vue'
import PackageDetailDrawer from '@/components/PackageDetailDrawer.vue'
import { formatBytesShort, formatCount } from '@/utils/format'

const props = defineProps<{ manager: ManagerInfo }>()

/** 把右键菜单里的真实操作请求向上转发（确认对话框由父级持有） */
const emit = defineEmits<{ operate: [PackageRecord, ManagementAction] }>()

const store = useAppStore()
const { t } = useI18n()

type SortKey = 'name' | 'size'

const sortKey = ref<SortKey>('name')
const sortAsc = ref(true)
const detailTarget = ref<PackageRecord | null>(null)
const menuState = ref<{ record: PackageRecord; x: number; y: number } | null>(null)

/** 该管理器的包（再用全局关键字过滤，兼容「从搜索跳过来」的场景） */
const rows = computed(() => {
  const keyword = store.keyword.trim().toLowerCase()
  const list = (store.report?.packages ?? []).filter((p) => {
    if (p.manager !== props.manager.id) return false
    if (!keyword) return true
    return (
      p.name.toLowerCase().includes(keyword) || (p.version ?? '').toLowerCase().includes(keyword)
    )
  })
  const dir = sortAsc.value ? 1 : -1
  list.sort((a, b) =>
    sortKey.value === 'size'
      ? ((a.size ?? 0) - (b.size ?? 0)) * dir
      : a.name.localeCompare(b.name) * dir,
  )
  return list
})

const hasSize = computed(() => rows.value.some((r) => r.size !== null))

function toggleSort(key: SortKey) {
  if (sortKey.value === key) sortAsc.value = !sortAsc.value
  else {
    sortKey.value = key
    sortAsc.value = true
  }
}

function sortIndicator(key: SortKey) {
  return sortKey.value === key ? (sortAsc.value ? ' ▲' : ' ▼') : ''
}

function openMenu(record: PackageRecord, event: MouseEvent) {
  menuState.value = { record, x: event.clientX, y: event.clientY }
}
</script>

<template>
  <div class="panel">
    <div class="panel__head">
      <span class="panel__title">{{ t('manager.packages') }}</span>
      <span class="tag">{{ formatCount(rows.length) }}</span>
      <input
        v-model="store.keyword"
        class="input input--search"
        type="search"
        :placeholder="t('packages.search')"
        style="margin-left: auto"
      />
    </div>

    <div v-if="rows.length === 0" class="empty" style="padding: 40px 24px">
      <div class="empty__icon">📦</div>
      <div>{{ store.keyword ? t('packages.noMatch') : t('packages.emptyTitle') }}</div>
      <div class="hint">{{ store.keyword ? t('packages.noMatchHint') : t('packages.emptyHint') }}</div>
    </div>

    <div v-else class="table-wrap" style="max-height: calc(100vh - 320px)">
      <table class="data pkg-table">
        <thead>
          <tr>
            <th class="is-sortable" @click="toggleSort('name')">
              {{ t('packages.name') }} / {{ t('packages.version') }}{{ sortIndicator('name') }}
            </th>
            <th
              v-if="hasSize"
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
            <td>
              <div class="pkg-cell">
                <span class="pkg-cell__name" :title="p.description ?? p.name">{{ p.name }}</span>
                <span class="pkg-cell__version mono">{{ p.version ?? '—' }}</span>
                <span v-if="p.redundant" class="tag tag--warn">{{ t('packages.redundantMark') }}</span>
              </div>
            </td>
            <td v-if="hasSize" class="num">
              {{ p.size === null ? '—' : formatBytesShort(p.size) }}
            </td>
            <td>
              <button class="btn btn--ghost btn--sm" @click="detailTarget = p">
                {{ t('detail.title') }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="panel__head" style="border-top: 1px solid var(--border-subtle); border-bottom: none">
      <span class="hint">在包上<strong>右键</strong>可打开管理菜单（更新 / 卸载 / 安装为占位按钮）。</span>
    </div>
  </div>

  <PackageContextMenu
    v-if="menuState"
    :record="menuState.record"
    :x="menuState.x"
    :y="menuState.y"
    @close="menuState = null"
    @inspect="detailTarget = $event"
    @manage="detailTarget = $event"
    @operate="(record, action) => emit('operate', record, action)"
  />

  <PackageDetailDrawer :record="detailTarget" @close="detailTarget = null" />
</template>
