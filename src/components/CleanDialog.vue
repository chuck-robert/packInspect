<script setup lang="ts">
/**
 * 清理对话框 —— 强制三段式安全流程：
 *   1. 勾选候选（受保护项不可选）
 *   2. 预览（后端 dry-run，只算账不删除）
 *   3. 二次确认后才真正删除（≥1GB 需手打 DELETE）
 *
 * 组件不直接调用底层 IPC，全部经 store 的状态机（cleanPhase）驱动。
 */
import { computed, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { CleanCandidate } from '@/types'
import { formatBytes, formatCount, ellipsisPath } from '@/utils/format'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [boolean] }>()

const store = useAppStore()
const { t } = useI18n()

const checked = ref<Set<string>>(new Set())
const confirmText = ref('')
const filterManager = ref<string | 'all'>('all')

/** 需要手打确认的体积阈值 */
const CONFIRM_THRESHOLD = 1024 * 1024 * 1024

const candidates = computed(() => {
  const list = store.candidates
  return filterManager.value === 'all'
    ? list
    : list.filter((c) => c.managerId === filterManager.value)
})

const managersInList = computed(() => [...new Set(store.candidates.map((c) => c.managerId))])

const selectedBytes = computed(() =>
  checked.value.size
    ? candidates.value.filter((c) => checked.value.has(c.id)).reduce((sum, c) => sum + c.bytes, 0)
    : 0,
)

const needsTypedConfirm = computed(
  () => selectedBytes.value >= CONFIRM_THRESHOLD && confirmText.value.trim() !== 'DELETE',
)

const previewOk = computed(() => store.cleanPhase === 'previewed')
const previewFreed = computed(() =>
  store.previewResults.filter((r) => r.ok).reduce((sum, r) => sum + r.freedBytes, 0),
)
const failedCount = computed(() => store.previewResults.filter((r) => !r.ok).length)

function close() {
  emit('update:open', false)
}

function toggle(candidate: CleanCandidate) {
  if (candidate.protected) return
  const next = new Set(checked.value)
  if (next.has(candidate.id)) next.delete(candidate.id)
  else next.add(candidate.id)
  checked.value = next
  if (store.cleanPhase === 'previewed' || store.cleanPhase === 'done') store.resetClean()
}

function toggleAll(payload: boolean) {
  const next = new Set<string>()
  if (payload) {
    for (const candidate of candidates.value) if (!candidate.protected) next.add(candidate.id)
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
    <div class="modal" style="width: min(900px, 100%)">
      <div class="modal__head">
        <div class="modal__title">{{ t('clean.title') }}</div>
        <span class="tag tag--ok">{{ t('clean.safeMode') }}</span>
        <button class="btn btn--ghost btn--sm" @click="close">✕</button>
      </div>

      <div class="modal__body">
        <div class="banner banner--info" style="margin-bottom: 12px">{{ t('clean.warning') }}</div>

        <!-- 阶段：预览结果 -->
        <template v-if="previewOk">
          <div class="stat-grid" style="margin-bottom: 12px">
            <div class="stat">
              <div class="stat__label">{{ t('clean.willDelete') }}</div>
              <div class="stat__value">{{ store.previewResults.filter((r) => r.ok).length }}</div>
            </div>
            <div class="stat">
              <div class="stat__label">{{ t('clean.willFree') }}</div>
              <div class="stat__value">{{ formatBytes(previewFreed) }}</div>
            </div>
          </div>

          <table class="data" style="margin-bottom: 12px">
            <thead>
              <tr>
                <th>{{ t('clean.path') }}</th>
                <th class="num" style="width: 100px">{{ t('clean.size') }}</th>
                <th style="width: 220px">{{ t('clean.note') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="result in store.previewResults" :key="result.candidateId">
                <td class="path-cell" :title="result.path">{{ ellipsisPath(result.path, 58) }}</td>
                <td class="num">{{ formatBytes(result.freedBytes) }}</td>
                <td :class="result.ok ? 'hint' : 'danger-text'">{{ result.message }}</td>
              </tr>
            </tbody>
          </table>

          <div v-if="selectedBytes >= CONFIRM_THRESHOLD" class="banner banner--warn">
            {{ t('clean.typedConfirm') }}
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
              <option value="all">{{ t('clean.filterAll') }}</option>
              <option v-for="manager in managersInList" :key="manager" :value="manager">
                {{ manager }}
              </option>
            </select>
            <button class="btn btn--sm" @click="toggleAll(true)">{{ t('clean.selectAll') }}</button>
            <button class="btn btn--sm" @click="toggleAll(false)">{{ t('clean.clearAll') }}</button>
            <span class="banner__spacer" />
            <span v-if="store.loadingCandidates" class="row hint">
              <span class="spinner" /> {{ t('clean.scanning') }}
            </span>
            <span v-else class="hint">{{ t('clean.candidates', { count: candidates.length }) }}</span>
            <button class="btn btn--ghost btn--sm" @click="store.loadCandidates()">
              {{ t('clean.rescan') }}
            </button>
          </div>

          <div v-if="!store.loadingCandidates && candidates.length === 0" class="empty">
            <div class="empty__icon">✨</div>
            <div>{{ t('clean.none') }}</div>
          </div>

          <div v-else class="table-wrap" style="max-height: 340px">
            <table class="data">
              <thead>
                <tr>
                  <th style="width: 36px" />
                  <th>{{ t('clean.path') }}</th>
                  <th style="width: 100px">{{ t('clean.type') }}</th>
                  <th class="num" style="width: 88px">{{ t('clean.size') }}</th>
                  <th style="width: 86px">{{ t('clean.risk') }}</th>
                  <th>{{ t('clean.note') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="candidate in candidates"
                  :key="candidate.id"
                  :class="{ 'is-selected': checked.has(candidate.id) }"
                >
                  <td>
                    <input
                      type="checkbox"
                      :checked="checked.has(candidate.id)"
                      :disabled="candidate.protected"
                      @change="toggle(candidate)"
                    />
                  </td>
                  <td class="path-cell" :title="candidate.path">
                    {{ ellipsisPath(candidate.path, 52) }}
                  </td>
                  <td>
                    <span class="tag">{{ t(`clean.kind.${candidate.kind}`) }}</span>
                  </td>
                  <td class="num">{{ formatBytes(candidate.bytes) }}</td>
                  <td>
                    <span class="tag" :class="riskClass(candidate.risk)">
                      {{ t(`clean.risk.${candidate.risk}`) }}
                    </span>
                  </td>
                  <td class="hint">{{ candidate.reason }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </template>

        <!-- 执行结果 -->
        <template v-if="store.cleanPhase === 'done'">
          <div class="banner banner--success" style="margin-top: 12px">
            {{ t('clean.done', { size: formatBytes(previewFreed) }) }}
            <span v-if="failedCount" class="danger-text">
              {{ t('clean.someFailed', { count: formatCount(failedCount) }) }}
            </span>
          </div>
        </template>
      </div>

      <div class="modal__foot">
        <span class="hint" style="flex: 1">
          <template v-if="checked.size">
            {{ t('clean.selected', { count: checked.size, size: formatBytes(selectedBytes) }) }}
          </template>
          <template v-else>{{ t('clean.pickHint') }}</template>
        </span>

        <button class="btn" @click="close">{{ t('clean.close') }}</button>

        <template v-if="previewOk">
          <button class="btn" @click="store.resetClean()">{{ t('clean.back') }}</button>
          <button
            class="btn btn--danger"
            :disabled="needsTypedConfirm || store.cleanPhase === 'executing'"
            @click="runExecute"
          >
            {{ store.cleanPhase === 'executing' ? t('clean.deleting') : t('clean.confirm') }}
          </button>
        </template>

        <button
          v-else
          class="btn btn--primary"
          :disabled="checked.size === 0 || store.cleanPhase === 'previewing'"
          @click="runPreview"
        >
          {{ store.cleanPhase === 'previewing' ? t('clean.calculating') : t('clean.preview') }}
        </button>
      </div>
    </div>
  </div>
</template>
