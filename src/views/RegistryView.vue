<script setup lang="ts">
/**
 * 镜像源视图：查看当前配置 + 安全修改（修改前预览 diff，保存时后端自动备份原文件）。
 * 只对支持源配置的管理器开放；其它管理器给出明确提示而不是空面板。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import { api, IpcError } from '@/api'
import type { RegistryConfig } from '@/types'
import { ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const editing = ref(false)
const editKey = ref('registry')
const editValue = ref('')
const preview = ref('')
const previewError = ref('')
const showRaw = ref(false)

const registry = computed<RegistryConfig | null>(() => store.activeRegistry)

/** 支持源配置的管理器（与后端 `manager::supports_registry` 对应） */
const REGISTRY_MANAGERS = [
  'npm',
  'pnpm',
  'yarn',
  'pip',
  'cargo',
  'go',
  'composer',
  'gem',
  'conda',
  'maven',
]

const KEY_PRESETS: Record<string, string[]> = {
  npm: ['registry', 'strict-ssl', 'proxy', 'https-proxy'],
  pnpm: ['registry', 'strict-ssl', 'proxy'],
  yarn: ['registry', 'strict-ssl'],
  pip: ['index-url', 'extra-index-url', 'trusted-host', 'timeout'],
  cargo: ['registry', 'index'],
  go: ['GOPROXY', 'GOSUMDB'],
  composer: ['registry'],
  gem: ['sources'],
  conda: ['channels'],
  maven: ['registry'],
}

const supportsRegistry = computed(() =>
  store.activeManager ? REGISTRY_MANAGERS.includes(store.activeManager) : false,
)

const availableKeys = computed(() => {
  const id = store.activeManager ?? ''
  return KEY_PRESETS[id] ?? ['registry']
})

/** 常用镜像源快捷填入 */
const QUICK_URLS = computed(() => {
  const id = store.activeManager ?? ''
  if (['npm', 'pnpm', 'yarn'].includes(id)) {
    return [
      { label: 'npm 官方', value: 'https://registry.npmjs.org/' },
      { label: '淘宝镜像', value: 'https://registry.npmmirror.com/' },
      { label: '华为云', value: 'https://mirrors.huaweicloud.com/repository/npm/' },
    ]
  }
  if (id === 'pip') {
    return [
      { label: 'PyPI 官方', value: 'https://pypi.org/simple' },
      { label: '清华镜像', value: 'https://pypi.tuna.tsinghua.edu.cn/simple' },
      { label: '阿里云', value: 'https://mirrors.aliyun.com/pypi/simple/' },
    ]
  }
  if (id === 'go') {
    return [
      { label: '官方', value: 'https://proxy.golang.org,direct' },
      { label: '七牛', value: 'https://goproxy.cn,direct' },
    ]
  }
  if (id === 'cargo') {
    return [
      { label: '官方', value: 'https://crates.io/' },
      { label: '中科大', value: 'https://mirrors.ustc.edu.cn/crates.io-index' },
    ]
  }
  return []
})

onMounted(async () => {
  if (store.activeManager && supportsRegistry.value && !registry.value) {
    await store.loadRegistry(store.activeManager)
  }
})

watch(
  () => store.activeManager,
  async (id) => {
    editing.value = false
    preview.value = ''
    previewError.value = ''
    if (!id) return
    editKey.value = availableKeys.value[0]
    if (supportsRegistry.value && !store.managers.find((m) => m.id === id)?.registry) {
      await store.loadRegistry(id)
    }
  },
)

/** 预览：完全在后端计算，前端不拼接配置文件 */
async function refreshPreview() {
  previewError.value = ''
  const id = store.activeManager
  if (!id) return
  try {
    preview.value = await api.previewRegistryChange(
      id,
      editKey.value,
      editValue.value,
      registry.value?.raw ?? '',
    )
  } catch (e) {
    const err = e instanceof IpcError ? e : IpcError.from(e)
    preview.value = ''
    previewError.value = err.message
  }
}

watch([editKey, editValue], () => {
  if (editing.value) void refreshPreview()
})

function startEdit(entry?: { key: string; value: string }) {
  editing.value = true
  editKey.value = entry?.key ?? availableKeys.value[0]
  editValue.value = entry?.value ?? ''
  preview.value = ''
  previewError.value = ''
  void refreshPreview()
}

async function save() {
  const id = store.activeManager
  if (!id) return
  const ok = await store.saveRegistry(id, editKey.value, editValue.value)
  if (ok) {
    editing.value = false
    preview.value = ''
  }
}

function diffLines(text: string) {
  return text.split('\n').map((line, i) => ({ line, i }))
}
</script>

<template>
  <section class="scroll-area" style="padding: 12px 14px">
    <div v-if="!store.activeManager" class="empty">
      <div class="empty__icon">🌐</div>
      <div>{{ t('registry.pickManager') }}</div>
      <div class="hint">{{ t('registry.pickManagerHint') }}</div>
    </div>

    <div v-else-if="!supportsRegistry" class="empty">
      <div class="empty__icon">—</div>
      <div>{{ t('registry.noRegistry') }}</div>
      <div class="hint">{{ store.activeManager }}</div>
    </div>

    <div v-else-if="!registry" class="empty">
      <div class="empty__icon">🌐</div>
      <div>{{ t('registry.loading', { name: store.activeManager }) }}</div>
      <button class="btn btn--sm" @click="store.loadRegistry(store.activeManager)">
        {{ t('registry.reload') }}
      </button>
    </div>

    <div v-else class="col">
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">
            {{ t('registry.title', { name: registry.managerId }) }}
          </span>
          <span class="tag" :class="registry.writable ? 'tag--ok' : 'tag--warn'">
            {{ registry.writable ? t('registry.writable') : t('registry.readonly') }}
          </span>
          <span class="panel__spacer" />
          <button class="btn btn--sm" @click="showRaw = !showRaw">
            {{ showRaw ? t('registry.hideRaw') : t('registry.showRaw') }}
          </button>
          <button class="btn btn--primary btn--sm" @click="startEdit()">
            {{ t('registry.edit') }}
          </button>
        </div>

        <div class="panel__body">
          <table class="data">
            <thead>
              <tr>
                <th style="width: 180px">{{ t('registry.key') }}</th>
                <th>{{ t('registry.value') }}</th>
                <th style="width: 180px">{{ t('registry.source') }}</th>
                <th style="width: 60px" />
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in registry.entries" :key="entry.key">
                <td class="mono">{{ entry.key }}</td>
                <td class="mono">{{ entry.value }}</td>
                <td>
                  <span class="tag" :class="entry.userDefined ? 'tag--accent' : ''">
                    {{ entry.userDefined ? t('registry.userDefined') : t('registry.builtin') }}
                  </span>
                  <span v-if="entry.hint" class="hint" style="margin-left: 6px">{{ entry.hint }}</span>
                </td>
                <td>
                  <button class="btn btn--ghost btn--sm" @click="startEdit(entry)">
                    {{ t('registry.change') }}
                  </button>
                </td>
              </tr>
            </tbody>
          </table>

          <div v-if="showRaw" style="margin-top: 12px">
            <div class="hint" style="margin-bottom: 6px">
              {{ ellipsisPath(store.managers.find((m) => m.id === store.activeManager)?.configFile ?? '', 90) }}
            </div>
            <pre class="diff">{{ registry.raw || t('registry.rawMissing') }}</pre>
          </div>
        </div>
      </div>

      <!-- 编辑面板 -->
      <div v-if="editing" class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ t('registry.editTitle') }}</span>
          <span class="panel__spacer" />
          <button class="btn btn--ghost btn--sm" @click="editing = false">
            {{ t('registry.cancel') }}
          </button>
        </div>

        <div class="panel__body col">
          <div class="row">
            <label class="hint" style="width: 80px">{{ t('registry.key') }}</label>
            <select v-model="editKey" class="select" style="width: 220px">
              <option v-for="key in availableKeys" :key="key" :value="key">{{ key }}</option>
            </select>
          </div>

          <div class="row">
            <label class="hint" style="width: 80px">{{ t('registry.valueLabel') }}</label>
            <input
              v-model="editValue"
              class="input input--mono"
              style="flex: 1"
              placeholder="https://..."
              spellcheck="false"
            />
          </div>

          <div v-if="QUICK_URLS.length" class="row" style="flex-wrap: wrap">
            <span class="hint">{{ t('registry.quickFill') }}</span>
            <button
              v-for="quick in QUICK_URLS"
              :key="quick.value"
              class="btn btn--sm"
              @click="editValue = quick.value"
            >
              {{ quick.label }}
            </button>
          </div>

          <div v-if="previewError" class="banner banner--error">{{ previewError }}</div>

          <div v-else-if="preview">
            <div class="hint" style="margin-bottom: 6px">{{ t('registry.previewLabel') }}</div>
            <pre class="diff"><span
              v-for="line in diffLines(preview)"
              :key="line.i"
              class="diff__line--add"
            >{{ line.line }}
</span></pre>
          </div>

          <div class="banner banner--warn">{{ t('registry.saveNote') }}</div>

          <div class="row" style="justify-content: flex-end">
            <button class="btn" @click="editing = false">{{ t('registry.cancel') }}</button>
            <button
              class="btn btn--primary"
              :disabled="store.savingRegistry || !!previewError || !editValue"
              @click="save"
            >
              {{ store.savingRegistry ? t('registry.saving') : t('registry.save') }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
