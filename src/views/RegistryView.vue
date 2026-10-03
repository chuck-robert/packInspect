<script setup lang="ts">
/**
 * 镜像源视图。
 *
 * 交互（按需求调整）：
 * - **默认列出全部包管理器**，不需要先去侧边栏选一个再回来
 * - 左侧列表选择管理器，右侧显示它的镜像源配置与当前生效地址
 * - 只有支持源配置的生态可选中；其余显示为不可用并说明原因
 *
 * 安全：写回配置由后端完成（先备份原文件、只改目标行、原子替换），
 * 前端只传 `{ managerId, key, value }`，不拼配置文件内容。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import { api, IpcError } from '@/api'
import type { ManagerInfo, RegistryConfig } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'
import { ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const editing = ref(false)
const editKey = ref('registry')
const editValue = ref('')
const preview = ref('')
const previewError = ref('')
const showRaw = ref(false)

/** 支持镜像源配置的生态（与后端 `manager::supports_registry` 对应） */
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

/** 全部管理器：支持配置的排前面，其次已安装的 */
const managers = computed<ManagerInfo[]>(() =>
  [...store.managers].sort((a, b) => {
    const aOk = REGISTRY_MANAGERS.includes(a.id) ? 0 : 1
    const bOk = REGISTRY_MANAGERS.includes(b.id) ? 0 : 1
    return aOk - bOk || Number(b.detected) - Number(a.detected) || a.name.localeCompare(b.name)
  }),
)

const selectedId = ref<string | null>(null)
const selected = computed<ManagerInfo | null>(
  () => managers.value.find((m) => m.id === selectedId.value) ?? null,
)
const registry = computed<RegistryConfig | null>(() => selected.value?.registry ?? null)
const supportsRegistry = computed(() =>
  selectedId.value ? REGISTRY_MANAGERS.includes(selectedId.value) : false,
)

const availableKeys = computed(() => KEY_PRESETS[selectedId.value ?? ''] ?? ['registry'])

/** 常用镜像源快捷填入 */
const QUICK_URLS = computed(() => {
  const id = selectedId.value ?? ''
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

/** 当前生效的主源 */
const primaryEntry = computed(() => registry.value?.entries[0] ?? null)

async function select(id: string) {
  selectedId.value = id
  editing.value = false
  preview.value = ''
  previewError.value = ''
  showRaw.value = false
  if (REGISTRY_MANAGERS.includes(id)) {
    await store.loadRegistry(id)
    editKey.value = availableKeys.value[0]
  }
}

onMounted(async () => {
  // 默认选中：优先侧边栏当前范围 → 否则第一个支持配置的管理器
  const preferred =
    (store.activeManager && REGISTRY_MANAGERS.includes(store.activeManager)
      ? store.activeManager
      : null) ??
    managers.value.find((m) => REGISTRY_MANAGERS.includes(m.id))?.id ??
    null
  if (preferred) await select(preferred)
})

watch(
  () => store.activeManager,
  (id) => {
    if (id && REGISTRY_MANAGERS.includes(id) && id !== selectedId.value) void select(id)
  },
)

/** 预览：完全在后端计算，前端不拼接配置文件 */
async function refreshPreview() {
  previewError.value = ''
  const id = selectedId.value
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
  const id = selectedId.value
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
    <div class="registry-layout">
      <!-- 左：全部包管理器 -->
      <div class="panel registry-list">
        <div class="panel__head">
          <span class="panel__title">{{ t('registry.pickManager') }}</span>
          <span class="panel__spacer" />
          <span class="tag">{{ managers.length }}</span>
        </div>
        <div class="registry-list__body">
          <button
            v-for="manager in managers"
            :key="manager.id"
            class="registry-item"
            :class="{
              'is-active': manager.id === selectedId,
              'is-unsupported': !REGISTRY_MANAGERS.includes(manager.id),
            }"
            @click="select(manager.id)"
          >
            <ManagerLogo :manager-id="manager.id" :name="manager.name" size="sm" />
            <span class="registry-item__name">{{ manager.name }}</span>
            <span
              v-if="REGISTRY_MANAGERS.includes(manager.id)"
              class="dot"
              :class="manager.detected ? 'dot--ok' : 'dot--missing'"
            />
            <span v-else class="hint" style="font-size: 10px">{{ t('registry.na') }}</span>
          </button>
        </div>
      </div>

      <!-- 右：选中管理器的配置 -->
      <div class="col" style="min-width: 0; flex: 1">
        <div v-if="!selected" class="empty">
          <div class="empty__icon">🌐</div>
          <div>{{ t('registry.pickManagerHint') }}</div>
        </div>

        <div v-else-if="!supportsRegistry" class="empty">
          <div class="empty__icon">—</div>
          <div>{{ t('registry.noRegistry') }}</div>
          <div class="hint">{{ selected.name }}</div>
        </div>

        <template v-else>
          <div class="panel">
            <div class="panel__head">
              <ManagerLogo :manager-id="selected.id" :name="selected.name" size="sm" />
              <span class="panel__title">{{ t('registry.title', { name: selected.name }) }}</span>
              <span class="tag" :class="registry?.writable ? 'tag--ok' : 'tag--warn'">
                {{ registry?.writable ? t('registry.writable') : t('registry.readonly') }}
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
              <!-- 当前生效源：一眼能看到 -->
              <div v-if="primaryEntry" class="banner banner--info" style="margin-bottom: 12px">
                <span>{{ t('registry.current') }}</span>
                <code class="mono">{{ primaryEntry.value }}</code>
                <span v-if="primaryEntry.hint" class="tag tag--accent">{{ primaryEntry.hint }}</span>
              </div>

              <table class="data">
                <thead>
                  <tr>
                    <th style="width: 180px">{{ t('registry.key') }}</th>
                    <th>{{ t('registry.value') }}</th>
                    <th style="width: 170px">{{ t('registry.source') }}</th>
                    <th style="width: 56px" />
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="entry in registry?.entries ?? []" :key="entry.key">
                    <td class="mono">{{ entry.key }}</td>
                    <td class="mono">{{ entry.value }}</td>
                    <td>
                      <span class="tag" :class="entry.userDefined ? 'tag--accent' : ''">
                        {{ entry.userDefined ? t('registry.userDefined') : t('registry.builtin') }}
                      </span>
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
                  {{ ellipsisPath(selected.configFile ?? '', 90) }}
                </div>
                <pre class="diff">{{ registry?.raw || t('registry.rawMissing') }}</pre>
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
        </template>
      </div>
    </div>
  </section>
</template>
