<script setup lang="ts">
/**
 * 已安装包视图：搜索 / 过滤 / 排序 / 详情。
 * 数据全部来自 store，组件本身不做任何 IPC 调用（除详情里的「复制路径」用剪贴板 API）。
 */
import { computed, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import type { PackageRecord } from '@/types'
import { SCOPE_LABELS, formatBytesShort, formatCount, ellipsisPath } from '@/utils/format'

type SortKey = 'name' | 'version' | 'manager' | 'size'

const store = useAppStore()
const sortKey = ref<SortKey>('name')
const sortAsc = ref(true)
const selected = ref<PackageRecord | null>(null)

const rows = computed(() => {
  const list = [...store.visiblePackages]
  const dir = sortAsc.value ? 1 : -1
  list.sort((a, b) => {
    switch (sortKey.value) {
      case 'size':
        return ((a.size ?? 0) - (b.size ?? 0)) * dir
      case 'version':
        return (a.version ?? '').localeCompare(b.version ?? '', undefined, { numeric: true }) * dir
      case 'manager':
        return a.manager.localeCompare(b.manager) * dir
      default:
        return a.name.localeCompare(b.name) * dir
    }
  })
  return list
})

const redundantCount = computed(
  () => store.visiblePackages.filter((p) => p.redundant).length,
)

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

async function copyPath(path: string | null) {
  if (!path) return
  try {
    await navigator.clipboard.writeText(path)
    store.notify('info', '路径已复制到剪贴板')
  } catch {
    store.notify('warn', '复制失败，请手动选择文本')
  }
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
        placeholder="搜索包名 / 版本 / 路径"
      />
      <label class="checkbox">
        <input v-model="store.onlyRedundant" type="checkbox" />
        只看冗余项
      </label>
      <span v-if="redundantCount" class="tag tag--warn">冗余 {{ redundantCount }}</span>
      <span class="tag">{{ formatCount(rows.length) }} 项</span>
      <span class="banner__spacer" />
      <span v-if="store.measuredBytes > 0" class="hint">
        已统计体积合计：{{ formatBytesShort(store.measuredBytes) }}
      </span>
    </div>

    <!-- 空态 -->
    <div v-if="!store.report" class="empty">
      <div class="empty__icon">📦</div>
      <div>还没有扫描数据</div>
      <div class="hint">
        点击右上角「开始扫描」。首次扫描只读取目录，勾选「统计包体积」会额外遍历每个包目录，速度较慢。
      </div>
    </div>

    <div v-else-if="rows.length === 0" class="empty">
      <div class="empty__icon">🔍</div>
      <div>没有匹配的包</div>
      <div class="hint">试试清空搜索关键字，或切换到「全部」范围。</div>
    </div>

    <!-- 表格 -->
    <div v-else class="panel">
      <div class="table-wrap" style="max-height: calc(100vh - 250px)">
        <table class="data">
          <thead>
            <tr>
              <th class="is-sortable" @click="toggleSort('name')">包名{{ sortIndicator('name') }}</th>
              <th class="is-sortable" style="width: 130px" @click="toggleSort('version')">
                版本{{ sortIndicator('version') }}
              </th>
              <th class="is-sortable" style="width: 96px" @click="toggleSort('manager')">
                来源{{ sortIndicator('manager') }}
              </th>
              <th style="width: 84px">作用域</th>
              <th class="is-sortable num" style="width: 92px" @click="toggleSort('size')">
                体积{{ sortIndicator('size') }}
              </th>
              <th>安装路径</th>
              <th style="width: 62px" />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="p in rows"
              :key="`${p.manager}:${p.name}:${p.version}:${p.path}`"
              :class="{ 'is-redundant': p.redundant }"
            >
              <td :title="p.description ?? p.name">
                {{ p.name }}
                <span v-if="p.redundant" class="tag tag--warn" style="margin-left: 6px">旧版本</span>
              </td>
              <td class="mono">{{ p.version ?? '—' }}</td>
              <td><span class="tag">{{ p.manager }}</span></td>
              <td>
                <span class="tag" :class="p.scope === 'global' ? 'tag--accent' : ''">
                  {{ SCOPE_LABELS[p.scope] }}
                </span>
              </td>
              <td class="num">{{ p.size === null ? '—' : formatBytesShort(p.size) }}</td>
              <td class="path-cell" :title="p.path ?? ''">{{ ellipsisPath(p.path, 56) }}</td>
              <td>
                <button class="btn btn--ghost btn--sm" @click="selected = p">详情</button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- 详情抽屉（简化版弹层） -->
    <div v-if="selected" class="modal-backdrop" @click.self="selected = null">
      <div class="modal" style="width: min(720px, 100%)">
        <div class="modal__head">
          <div class="modal__title">{{ selected.name }}</div>
          <span class="tag">{{ selected.manager }}</span>
          <span class="tag">{{ SCOPE_LABELS[selected.scope] }}</span>
          <button class="btn btn--ghost btn--sm" @click="selected = null">✕</button>
        </div>
        <div class="modal__body">
          <div class="kv">
            <span class="kv__k">版本</span>
            <span class="kv__v">{{ selected.version ?? '—' }}</span>
            <span class="kv__k">安装路径</span>
            <span class="kv__v">{{ selected.path ?? '未识别' }}</span>
            <span class="kv__k">体积</span>
            <span class="kv__v">{{ selected.size === null ? '未统计' : formatBytesShort(selected.size) }}</span>
            <span class="kv__k">描述</span>
            <span class="kv__v">{{ selected.description ?? '—' }}</span>
            <span v-if="selected.redundant" class="kv__k">冗余原因</span>
            <span v-if="selected.redundant" class="kv__v danger-text">
              {{ selected.redundantReason }}
            </span>
          </div>
          <p class="hint" style="margin-top: 12px">
            本工具只读取包信息，不会卸载任何包。若要移除旧版本，请使用包管理器自身的
            <code>uninstall</code> 命令；「清理缓存」只会删除下载缓存。
          </p>
        </div>
        <div class="modal__foot">
          <button class="btn" :disabled="!selected.path" @click="copyPath(selected.path)">
            复制路径
          </button>
          <button class="btn btn--primary" @click="selected = null">关闭</button>
        </div>
      </div>
    </div>
  </section>
</template>
