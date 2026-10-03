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
import { formatBytesShort, formatCount, matchesKeyword } from '@/utils/format'

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

/**
 * 该管理器的包（再用全局关键字过滤）。
 *
 * 【必须与顶部搜索用同一套匹配规则】
 * 这里此前用的是普通 `includes`，只查 name 与 version；
 * 顶部搜索用的是 `matchesKeyword`（归一化 + 查 name / version / description / path）。
 * 两套规则不一致会造成「顶部搜得到、这里搜不到」的诡异现象 ——
 * 例如 winget 的 Oh My Posh：name 是机器 ID `JanDeDobbeleer.OhMyPosh`，
 * 显示名「Oh My Posh」只在 description 里，用户输入 `oh my posh`（带空格）时，
 * 普通 includes 匹配不上，而归一化匹配可以。现在统一走 matchesKeyword。
 */
const rows = computed(() => {
  const keyword = store.keyword.trim()
  const list = (store.report?.packages ?? []).filter((p) => {
    if (p.manager !== props.manager.id) return false
    return matchesKeyword([p.name, p.version, p.description, p.path], keyword)
  })
  const dir = sortAsc.value ? 1 : -1
  list.sort((a, b) =>
    sortKey.value === 'size'
      ? ((a.size ?? 0) - (b.size ?? 0)) * dir
      : a.name.localeCompare(b.name) * dir,
  )
  return list
})

/** 该管理器一共有多少包（不受关键字影响，用于空态提示） */
const totalInManager = computed(() =>
  (store.report?.packages ?? []).filter((p) => p.manager === props.manager.id).length,
)

const hasSize = computed(() => rows.value.some((r) => r.size !== null))

/** 当前正在扫描的就是这个管理器（用于区分「还在加载」与「搜不到」） */
const managerScanning = computed(
  () => store.scanning && store.scanProgress.current === props.manager.id,
)

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

    <!--
      空态分三种情况，不要混为一谈：
      1. 该管理器正在扫描 → 说明数据还没到，不是「搜不到」
      2. 有搜索词 → 才是真的没有匹配
      3. 既没扫也没搜 → 列表本来就是空的
    -->
    <div v-if="rows.length === 0" class="empty" style="padding: 40px 24px">
      <template v-if="managerScanning">
        <span class="spinner" />
        <div>{{ t('packages.loading') }}</div>
        <div class="hint">{{ t('packages.loadingHint') }}</div>
      </template>
      <template v-else>
        <div class="empty__icon">📦</div>
        <div>{{ store.keyword ? t('packages.noMatch') : t('packages.emptyTitle') }}</div>
        <div class="hint">
          {{ store.keyword ? t('packages.noMatchHint') : t('packages.emptyHint') }}
        </div>
        <!--
          显示匹配上下文：万一「顶部搜得到、这里搜不到」，这行能直接说明原因 ——
          是关键字没传过来，还是该管理器本来就没扫到包。
        -->
        <div v-if="store.keyword" class="hint mono" style="margin-top: 8px; opacity: 0.75">
          {{
            t('packages.matchContext', {
              keyword: store.keyword,
              manager: manager.id,
              total: formatCount(totalInManager),
            })
          }}
        </div>
      </template>
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
