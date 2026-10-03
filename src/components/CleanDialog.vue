<script setup lang="ts">
/**
 * 清理对话框 —— 强制三段式安全流程：
 *   1. 勾选候选（受保护项不可选）
 *   2. 预览（后端 dry-run，只算账不删除）
 *   3. 二次确认后才真正删除
 *
 * 组件不直接调用底层 IPC，全部经 store 的状态机（cleanPhase）驱动。
 */
import { computed, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import type { CleanCandidate } from '@/types'
import { CLEAN_KIND_LABELS, CLEAN_RISK_LABELS, formatBytes, formatCount, ellipsisPath } from '@/utils/format'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [boolean] }>()

const store = useAppStore()
const checked = ref<Set<string>>(new Set())
const confirmText = ref('')
const filterManager = ref<string | 'all'>('all')

/** 需要输入确认的阈值：超过这个体积必须手打 DELETE */
const CONFIRM_THRESHOLD = 1024 * 1024 * 1024

const candidates = computed(() => {
  const list = store.candidates
  return filterManager.value === 'all' ? list : list.filter((c) => c.managerId === filterManager.value)
})

const managersInList = computed(() => [...new Set(store.candidates.map((c) => c.managerId))])

const selectedBytes = computed(() =>
  checked.value.size
    ? candidates.value.filter((c) => checked.value.has(c.id)).reduce((s, c) => s + c.bytes, 0)
    : 0,
)

const needsTypedConfirm = computed(
  () => selectedBytes.value >= CONFIRM_THRESHOLD && confirmText.value.trim() !== 'DELETE',
)

const previewOk = computed(() => store.cleanPhase === 'previewed')
const previewFreed = computed(() =>
  store.previewResults.filter((r) => r.ok).reduce((s, r) => s + r.freedBytes, 0),
)

function close() {
  emit('update:open', false)
}

function toggle(c: CleanCandidate) {
  if (c.protected) return
  const next = new Set(checked.value)
  if (next.has(c.id)) next.delete(c.id)
  else next.add(c.id)
  checked.value = next
  // 选择变化后必须重新预览
  if (store.cleanPhase === 'previewed' || store.cleanPhase === 'done') store.resetClean()
}

function toggleAll(payload: boolean) {
  const next = new Set<string>()
  if (payload) {
    for (const c of candidates.value) if (!c.protected) next.add(c.id)
  }
  checked.value = next
  store.resetClean()
}

function riskClass(risk: string) {
  if (risk === 'safe') return 'tag--ok'
  if (risk === 'warn') return 'tag--warn'
  return 'tag--danger'
}

async function runPreview() {
  await store.previewClean([...checked.value])
}

async function runExecute() {
  await store.executeClean([...checked.value])
  checked.value = new Set()
  confirmText.value = ''
}

// 关闭时重置，避免下次打开残留旧选择
watch(
  () => props.open,
  (open) => {
    if (!open) {
      checked.value = new Set()
      confirmText.value = ''
      store.resetClean()
    } else if (store.candidates.length === 0 && !store.loadingCandidates) {
      void store.loadCandidates()
    }
  },
)
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="close">
    <div class="modal" style="width: min(880px, 100%)">
      <div class="modal__head">
        <div class="modal__title">清理缓存</div>
        <span class="tag tag--ok">安全模式</span>
        <button class="btn btn--ghost btn--sm" @click="close">✕</button>
      </div>

      <div class="modal__body">
        <div class="banner banner--info" style="margin-bottom: 12px">
          本操作<strong>只删除缓存与临时文件</strong>，不会卸载任何包，也不会触碰
          <code>node_modules</code> / <code>site-packages</code> / 虚拟环境。
        </div>

        <!-- 阶段：预览结果 -->
        <template v-if="previewOk">
          <div class="stat-grid" style="margin-bottom: 12px">
            <div class="stat">
              <div class="stat__label">将删除</div>
              <div class="stat__value">{{ store.previewResults.filter((r) => r.ok).length }} 项</div>
            </div>
            <div class="stat">
              <div class="stat__label">可释放空间</div>
              <div class="stat__value">{{ formatBytes(previewFreed) }}</div>
            </div>
          </div>

          <table class="data" style="margin-bottom: 12px">
            <thead>
              <tr>
                <th>路径</th>
                <th class="num" style="width: 100px">大小</th>
                <th style="width: 200px">说明</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in store.previewResults" :key="r.candidateId">
                <td class="path-cell" :title="r.path">{{ ellipsisPath(r.path, 58) }}</td>
                <td class="num">{{ formatBytes(r.freedBytes) }}</td>
                <td :class="r.ok ? 'hint' : 'danger-text'">{{ r.message }}</td>
              </tr>
            </tbody>
          </table>

          <div
            v-if="selectedBytes >= CONFIRM_THRESHOLD"
            class="banner banner--warn"
            style="margin-bottom: 10px"
          >
            本次清理体积较大（≥ 1 GB）。请输入 <code>DELETE</code> 以确认：
            <input
              v-model="confirmText"
              class="input input--mono"
              style="width: 140px; margin-left: 8px"
              placeholder="DELETE"
            />
          </div>
        </template>

        <!-- 阶段：选择候选 -->
        <template v-else>
          <div class="row" style="margin-bottom: 10px">
            <select v-model="filterManager" class="select">
              <option value="all">全部来源</option>
              <option v-for="m in managersInList" :key="m" :value="m">{{ m }}</option>
            </select>
            <button class="btn btn--sm" @click="toggleAll(true)">全选可清理项</button>
            <button class="btn btn--sm" @click="toggleAll(false)">清空选择</button>
            <span class="banner__spacer" />
            <span v-if="store.loadingCandidates" class="row hint">
              <span class="spinner" /> 正在统计缓存占用…
            </span>
            <span v-else class="hint">共 {{ candidates.length }} 项候选</span>
            <button class="btn btn--ghost btn--sm" @click="store.loadCandidates()">重新扫描</button>
          </div>

          <div v-if="!store.loadingCandidates && candidates.length === 0" class="empty">
            <div class="empty__icon">✨</div>
            <div>没有发现可清理的缓存</div>
          </div>

          <div v-else class="table-wrap" style="max-height: 340px">
            <table class="data">
              <thead>
                <tr>
                  <th style="width: 36px" />
                  <th>路径</th>
                  <th style="width: 92px">类型</th>
                  <th class="num" style="width: 88px">大小</th>
                  <th style="width: 76px">风险</th>
                  <th>说明</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="c in candidates"
                  :key="c.id"
                  :class="{ 'is-selected': checked.has(c.id) }"
                >
                  <td>
                    <input
                      type="checkbox"
                      :checked="checked.has(c.id)"
                      :disabled="c.protected"
                      @change="toggle(c)"
                    />
                  </td>
                  <td class="path-cell" :title="c.path">{{ ellipsisPath(c.path, 52) }}</td>
                  <td><span class="tag">{{ CLEAN_KIND_LABELS[c.kind] }}</span></td>
                  <td class="num">{{ formatBytes(c.bytes) }}</td>
                  <td>
                    <span class="tag" :class="riskClass(c.risk)">
                      {{ CLEAN_RISK_LABELS[c.risk] }}
                    </span>
                  </td>
                  <td class="hint">{{ c.reason }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </template>

        <!-- 执行结果 -->
        <template v-if="store.cleanPhase === 'done'">
          <div class="banner banner--success" style="margin-top: 12px">
            清理完成，共释放 {{ formatBytes(previewFreed) }}。
            <span v-if="store.previewResults.some((r) => !r.ok)" class="danger-text">
              有 {{ formatCount(store.previewResults.filter((r) => !r.ok).length) }} 项失败（多为文件被占用）。
            </span>
          </div>
        </template>
      </div>

      <div class="modal__foot">
        <span class="hint" style="flex: 1">
          <template v-if="checked.size">已选 {{ checked.size }} 项 / {{ formatBytes(selectedBytes) }}</template>
          <template v-else>请勾选要清理的项目</template>
        </span>

        <button class="btn" @click="close">关闭</button>

        <template v-if="previewOk">
          <button class="btn" @click="store.resetClean()">返回修改</button>
          <button
            class="btn btn--danger"
            :disabled="needsTypedConfirm || store.cleanPhase === 'executing'"
            @click="runExecute"
          >
            {{ store.cleanPhase === 'executing' ? '删除中…' : '确认删除' }}
          </button>
        </template>

        <button
          v-else
          class="btn btn--primary"
          :disabled="checked.size === 0 || store.cleanPhase === 'previewing'"
          @click="runPreview"
        >
          {{ store.cleanPhase === 'previewing' ? '计算中…' : '预览将删除的内容' }}
        </button>
      </div>
    </div>
  </div>
</template>
