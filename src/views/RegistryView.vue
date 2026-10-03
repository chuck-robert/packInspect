<script setup lang="ts">
/**
 * 镜像源视图：查看当前配置 + 安全修改（修改前预览 diff，保存时后端自动备份原文件）。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { api, IpcError } from '@/api'
import type { RegistryConfig } from '@/types'

const store = useAppStore()

const editing = ref(false)
const editKey = ref('registry')
const editValue = ref('')
const preview = ref('')
const previewError = ref('')
const showRaw = ref(false)

const registry = computed<RegistryConfig | null>(() => store.activeRegistry)

/** 可选配置键：按管理器给出预设 */
const KEY_PRESETS: Record<string, string[]> = {
  npm: ['registry', 'strict-ssl', 'proxy', 'https-proxy'],
  pnpm: ['registry', 'strict-ssl', 'proxy'],
  yarn: ['registry', 'strict-ssl'],
  bun: ['registry'],
  pip: ['index-url', 'extra-index-url', 'trusted-host', 'timeout'],
  uv: ['index-url', 'extra-index-url'],
  cargo: ['registry', 'index'],
  go: ['GOPROXY', 'GOSUMDB'],
  gem: ['sources'],
}

const availableKeys = computed(() => {
  const id = store.activeManager ?? ''
  return KEY_PRESETS[id] ?? ['registry']
})

/** 常用镜像源快捷填入 */
const QUICK_URLS = computed(() => {
  const id = store.activeManager ?? ''
  if (['npm', 'pnpm', 'yarn', 'bun'].includes(id)) {
    return [
      { label: 'npm 官方', value: 'https://registry.npmjs.org/' },
      { label: '淘宝镜像', value: 'https://registry.npmmirror.com/' },
      { label: '华为云', value: 'https://mirrors.huaweicloud.com/repository/npm/' },
    ]
  }
  if (['pip', 'uv'].includes(id)) {
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
  // 首屏若还没加载过镜像源，按需拉取一次
  if (store.activeManager && !registry.value) {
    await store.loadRegistry(store.activeManager)
  }
})

watch(
  () => store.activeManager,
  async (id) => {
    editing.value = false
    preview.value = ''
    previewError.value = ''
    if (id && !store.managers.find((m) => m.id === id)?.registry) {
      await store.loadRegistry(id)
    }
    if (id) editKey.value = availableKeys.value[0]
  },
)

/** 预览：完全在后端计算，前端不拼接配置文件 */
async function refreshPreview() {
  previewError.value = ''
  const id = store.activeManager
  if (!id) return
  try {
    preview.value = await api.previewRegistryChange(id, editKey.value, editValue.value, registry.value?.raw ?? '')
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
      <div>请先在左侧选择一个包管理器</div>
      <div class="hint">镜像源配置是按管理器读取的，选择后即可查看与修改。</div>
    </div>

    <div v-else-if="!registry" class="empty">
      <div class="empty__icon">🌐</div>
      <div>正在读取 {{ store.activeManager }} 的配置…</div>
      <button class="btn btn--sm" @click="store.loadRegistry(store.activeManager)">重新读取</button>
    </div>

    <div v-else class="col">
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ registry.managerId }} 镜像源</span>
          <span class="tag" :class="registry.writable ? 'tag--ok' : 'tag--warn'">
            {{ registry.writable ? '可写' : '只读或未创建' }}
          </span>
          <span class="panel__spacer" />
          <button class="btn btn--sm" @click="showRaw = !showRaw">
            {{ showRaw ? '隐藏原文' : '查看配置文件原文' }}
          </button>
          <button class="btn btn--primary btn--sm" @click="startEdit()">修改配置</button>
        </div>

        <div class="panel__body">
          <table class="data">
            <thead>
              <tr>
                <th style="width: 180px">配置项</th>
                <th>当前值</th>
                <th style="width: 150px">来源</th>
                <th style="width: 70px" />
              </tr>
            </thead>
            <tbody>
              <tr v-for="e in registry.entries" :key="e.key">
                <td class="mono">{{ e.key }}</td>
                <td class="mono">{{ e.value }}</td>
                <td>
                  <span class="tag" :class="e.userDefined ? 'tag--accent' : ''">
                    {{ e.userDefined ? '用户配置' : '内置默认' }}
                  </span>
                  <span v-if="e.hint" class="hint" style="margin-left: 6px">{{ e.hint }}</span>
                </td>
                <td>
                  <button class="btn btn--ghost btn--sm" @click="startEdit(e)">改</button>
                </td>
              </tr>
            </tbody>
          </table>

          <div v-if="showRaw" style="margin-top: 12px">
            <div class="hint" style="margin-bottom: 6px">配置文件原文（只读展示）</div>
            <pre class="diff">{{ registry.raw || '（文件不存在，保存时将创建）' }}</pre>
          </div>
        </div>
      </div>

      <!-- 编辑面板 -->
      <div v-if="editing" class="panel">
        <div class="panel__head">
          <span class="panel__title">修改镜像源</span>
          <span class="panel__spacer" />
          <button class="btn btn--ghost btn--sm" @click="editing = false">取消</button>
        </div>

        <div class="panel__body col">
          <div class="row">
            <label class="hint" style="width: 70px">配置项</label>
            <select v-model="editKey" class="select" style="width: 200px">
              <option v-for="k in availableKeys" :key="k" :value="k">{{ k }}</option>
            </select>
          </div>

          <div class="row">
            <label class="hint" style="width: 70px">值</label>
            <input
              v-model="editValue"
              class="input input--mono"
              style="flex: 1"
              placeholder="https://..."
              spellcheck="false"
            />
          </div>

          <div v-if="QUICK_URLS.length" class="row" style="flex-wrap: wrap">
            <span class="hint">快捷填入：</span>
            <button
              v-for="q in QUICK_URLS"
              :key="q.value"
              class="btn btn--sm"
              @click="editValue = q.value"
            >
              {{ q.label }}
            </button>
          </div>

          <div v-if="previewError" class="banner banner--error">{{ previewError }}</div>

          <div v-else-if="preview">
            <div class="hint" style="margin-bottom: 6px">保存后的配置文件预览：</div>
            <pre class="diff"><span
              v-for="d in diffLines(preview)"
              :key="d.i"
              class="diff__line--add"
            >{{ d.line }}
</span></pre>
          </div>

          <div class="banner banner--warn">
            保存前会自动把原配置文件备份为
            <code>*.bak-时间戳</code>。本工具只修改你指定的这一行配置项，其余内容原样保留。
          </div>

          <div class="row" style="justify-content: flex-end">
            <button class="btn" @click="editing = false">取消</button>
            <button
              class="btn btn--primary"
              :disabled="store.savingRegistry || !!previewError || !editValue"
              @click="save"
            >
              {{ store.savingRegistry ? '保存中…' : '保存并备份原文件' }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
