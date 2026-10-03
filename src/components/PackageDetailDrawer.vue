<script setup lang="ts">
/**
 * 包详情抽屉：展示安装信息 + 包内插件 / 依赖树。
 * 右键菜单选「管理此包」或行内「详情」都会打开这里。
 */
import { onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { PackageRecord, PluginNode } from '@/types'
import { formatBytes, ellipsisPath } from '@/utils/format'
import ManagerLogo from '@/components/ManagerLogo.vue'

const props = defineProps<{ record: PackageRecord | null }>()
const emit = defineEmits<{ close: [] }>()

const store = useAppStore()
const { t } = useI18n()
const nodes = ref<PluginNode[]>([])
const loading = ref(false)
const expanded = ref<Set<string>>(new Set())

async function load(force = false) {
  const record = props.record
  if (!record) return
  loading.value = true
  try {
    nodes.value = await store.loadPlugins(record, force)
    // 依赖类节点默认展开，文件类折叠（避免一次铺满）
    expanded.value = new Set(
      nodes.value.filter((n) => n.nodeType === 'dependency' || n.nodeType === 'plugin').map((n) => n.name),
    )
  } finally {
    loading.value = false
  }
}

watch(
  () => props.record,
  (record) => {
    nodes.value = []
    if (record) void load()
  },
)

onMounted(() => {
  if (props.record) void load()
})

function nodeTypeLabel(type: PluginNode['nodeType']) {
  const map: Record<string, string> = {
    plugin: 'node.plugin',
    dependency: 'node.dependency',
    file: 'node.file',
    dir: 'node.dir',
    runtime: 'node.runtime',
  }
  return t(map[type] ?? 'node.file')
}

function nodeTypeClass(type: PluginNode['nodeType']) {
  switch (type) {
    case 'plugin':
      return 'tag--accent'
    case 'dependency':
      return ''
    case 'runtime':
      return 'tag--ok'
    default:
      return ''
  }
}

function toggle(name: string) {
  const next = new Set(expanded.value)
  if (next.has(name)) next.delete(name)
  else next.add(name)
  expanded.value = next
}

async function copyPath(path: string | null) {
  if (!path) return
  try {
    await navigator.clipboard.writeText(path)
    store.notify('info', t('detail.copied'))
  } catch {
    store.notify('warn', t('detail.copyFailed'))
  }
}

if (!store.report) {
  store.notify('info', '')
}
</script>

<template>
  <div v-if="record" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal" style="width: min(860px, 100%)">
      <div class="modal__head">
        <ManagerLogo :manager-id="record.manager" :name="record.manager" size="md" />
        <div class="modal__title">{{ record.name }}</div>
        <span class="tag">{{ record.manager }}</span>
        <span class="tag" :class="record.scope === 'global' ? 'tag--accent' : ''">
          {{ t(`scope.${record.scope}`) }}
        </span>
        <span v-if="record.redundant" class="tag tag--warn">{{ t('packages.redundantMark') }}</span>
        <button class="btn btn--ghost btn--sm" @click="emit('close')">✕</button>
      </div>

      <div class="modal__body">
        <div class="kv">
          <span class="kv__k">{{ t('detail.version') }}</span>
          <span class="kv__v">{{ record.version ?? '—' }}</span>

          <span class="kv__k">{{ t('detail.path') }}</span>
          <span class="kv__v">{{ record.path ?? '—' }}</span>

          <span class="kv__k">{{ t('detail.size') }}</span>
          <span class="kv__v">{{ record.size === null ? '—' : formatBytes(record.size) }}</span>

          <template v-if="record.description">
            <span class="kv__k">{{ t('detail.description') }}</span>
            <span class="kv__v">{{ record.description }}</span>
          </template>

          <template v-if="record.redundant">
            <span class="kv__k">{{ t('detail.redundantReason') }}</span>
            <span class="kv__v danger-text">{{ record.redundantReason }}</span>
          </template>
        </div>

        <!-- 包内子节点 -->
        <div class="section-title">
          <span>{{ t('detail.plugins') }}</span>
          <span v-if="nodes.length" class="tag">{{ nodes.length }}</span>
          <span class="banner__spacer" />
          <button class="btn btn--ghost btn--sm" :disabled="loading" @click="load(true)">
            {{ t('cache.refresh') }}
          </button>
        </div>

        <div v-if="loading" class="row hint" style="padding: 12px 0">
          <span class="spinner" /> {{ t('detail.pluginsLoading') }}
        </div>

        <div v-else-if="nodes.length === 0" class="empty" style="padding: 24px">
          <div>{{ t('detail.pluginsEmpty') }}</div>
          <div class="hint">
            该包没有声明依赖或扩展点，或后端暂时无法解析其内容结构。
          </div>
        </div>

        <ul v-else class="plugin-tree list-reset">
          <li v-for="node in nodes" :key="`${node.nodeType}:${node.name}:${node.path ?? ''}`">
            <div class="plugin-row" @click="toggle(node.name)">
              <span class="plugin-row__caret">{{ expanded.has(node.name) ? '▾' : '▸' }}</span>
              <span class="tag" :class="nodeTypeClass(node.nodeType)">{{ nodeTypeLabel(node.nodeType) }}</span>
              <span class="plugin-row__name mono">{{ node.name }}</span>
              <span v-if="node.version" class="plugin-row__version mono">{{ node.version }}</span>
              <span v-if="node.size !== null" class="plugin-row__size mono">
                {{ formatBytes(node.size) }}
              </span>
            </div>
            <div v-if="expanded.has(node.name)" class="plugin-detail">
              <div v-if="node.note" class="hint">{{ node.note }}</div>
              <div v-if="node.path" class="plugin-detail__path mono" :title="node.path">
                {{ ellipsisPath(node.path, 88) }}
                <button class="btn btn--ghost btn--sm" @click.stop="copyPath(node.path)">copy</button>
              </div>
            </div>
          </li>
        </ul>

        <p class="hint" style="margin-top: 14px">{{ t('detail.safetyNote') }}</p>
      </div>

      <div class="modal__foot">
        <button class="btn" :disabled="!record.path" @click="copyPath(record.path)">
          {{ t('detail.copyPath') }}
        </button>
        <button class="btn btn--primary" @click="emit('close')">{{ t('clean.close') }}</button>
      </div>
    </div>
  </div>
</template>
