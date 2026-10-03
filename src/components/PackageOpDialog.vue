<script setup lang="ts">
/**
 * 包管理操作确认对话框（更新 / 卸载 / 安装）。
 *
 * 这是**唯一**会把用户环境改掉的入口，因此设计上刻意做重：
 * 1. 明确显示将执行的确切命令（来自后端白名单模板，不是前端拼的）
 * 2. 破坏性操作（安装 / 卸载都会改动环境）用红色警示，并要求勾选「我已了解后果」
 * 3. 执行阶段显示 spinner，禁止重复提交
 * 4. 结果完整回显 stdout / stderr 与退出码，失败时用户能自行复核
 *
 * 三种用法（都走同一个后端命令 `run_package_op`，因此约束完全一致）：
 * - 右键已安装的包 → 更新 / 卸载 / 重装
 * - 「浏览 / 安装」分页 → 安装新包
 */
import { computed, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagementAction, PackageOpResult, PackageRecord } from '@/types'
import { formatDuration } from '@/utils/format'

const props = defineProps<{
  open: boolean
  /** 目标包；直接安装新包时用包名构造一条最小记录 */
  record: PackageRecord | null
  /** 动作描述；不传时按 action 生成 */
  action: ManagementAction | null
}>()

const emit = defineEmits<{ 'update:open': [boolean]; done: [] }>()

const store = useAppStore()
const { t } = useI18n()

const acknowledged = ref(false)
const running = ref(false)
const result = ref<PackageOpResult | null>(null)
const copyState = ref<'idle' | 'ok' | 'fail'>('idle')

const isDestructive = computed(() => props.action?.destructive ?? false)
const command = computed(() => props.action?.commandHint ?? '')

/**
 * 是否必须勾选「我已了解后果」才能执行。
 *
 * 只有**卸载**要勾选 —— 它会真的把你的东西删掉。
 * 安装与更新虽然也改动环境，但不会造成数据丢失，而且用户是主动点进来的，
 * 对话框本身已经起到了确认作用；强行再要求勾选只会变成走过场。
 */
const requiresAck = computed(() => props.action?.action === 'uninstall')
const canRun = computed(() => !requiresAck.value || acknowledged.value)

const title = computed(() => {
  if (!props.action) return ''
  const map: Record<string, string> = {
    update: 'menu.update',
    uninstall: 'menu.uninstall',
    install: 'menu.install',
  }
  return map[props.action.action] ? t(map[props.action.action]) : props.action.label
})

// 每次打开都重置状态，避免上次的结果串到这次
watch(
  () => props.open,
  (open) => {
    if (open) {
      acknowledged.value = false
      result.value = null
      running.value = false
      copyState.value = 'idle'
    }
  },
)

function close() {
  emit('update:open', false)
  if (result.value) emit('done')
}

async function run() {
  const record = props.record
  const action = props.action
  if (!record || !action || running.value) return

  running.value = true
  try {
    result.value = await store.runPackageOp(record.manager, record.name, action.action)
    if (result.value?.success) {
      store.notify('success', `${record.name}：${action.action} 完成`)
    } else {
      store.notify('warn', `${record.name}：${action.action} 未成功，详见结果`)
    }
  } finally {
    running.value = false
  }
}

async function copyLogPath() {
  if (!result.value?.logPath) return
  try {
    await navigator.clipboard.writeText(result.value.logPath)
    store.notify('info', t('browse.copied'))
  } catch {
    store.notify('warn', t('browse.copyFailed'))
  }
}

async function copyCommand() {
  try {
    await navigator.clipboard.writeText(command.value)
    copyState.value = 'ok'
  } catch {
    copyState.value = 'fail'
  }
  window.setTimeout(() => (copyState.value = 'idle'), 2000)
}
</script>

<template>
  <div v-if="open && record && action" class="modal-backdrop" @click.self="close">
    <div class="modal" style="width: min(760px, 100%)">
      <div class="modal__head">
        <div class="modal__title">{{ title }}</div>
        <span class="tag">{{ record.manager }}</span>
        <span v-if="isDestructive" class="tag tag--danger">{{ t('ops.destructive') }}</span>
        <button class="btn btn--ghost btn--sm" @click="close">✕</button>
      </div>

      <div class="modal__body col">
        <!-- 目标包 -->
        <div class="kv">
          <span class="kv__k">{{ t('ops.package') }}</span>
          <span class="kv__v">
            {{ record.name }} <span v-if="record.version">@{{ record.version }}</span>
          </span>
          <template v-if="record.description">
            <span class="kv__k">{{ t('detail.description') }}</span>
            <span class="kv__v">{{ record.description }}</span>
          </template>
        </div>

        <!-- 将执行的命令 -->
        <div>
          <div class="hint" style="margin-bottom: 5px">{{ t('ops.willRun') }}</div>
          <div class="ops-command">
            <code>{{ command }}</code>
            <button class="btn btn--sm" @click="copyCommand">
              {{ copyState === 'ok' ? '✓' : copyState === 'fail' ? '✗' : t('browse.copy') }}
            </button>
          </div>
          <div v-if="action.note" class="hint" style="margin-top: 5px">{{ action.note }}</div>
        </div>

        <div v-if="isDestructive" class="banner banner--error">
          {{ t('ops.destructiveWarning') }}
        </div>

        <!-- 未执行：确认区 -->
        <template v-if="!result">
          <label v-if="requiresAck" class="checkbox" style="align-items: flex-start">
            <input v-model="acknowledged" type="checkbox" />
            <span>{{ t('ops.acknowledge') }}</span>
          </label>
          <p class="hint" style="margin: 0">{{ t('ops.consoleNote') }}</p>
        </template>

        <!-- 已执行：结果 -->
        <template v-else>
          <div class="banner" :class="result.success ? 'banner--success' : 'banner--error'">
            <span>{{ result.message }}</span>
            <span class="banner__spacer" />
            <span class="mono">
              {{ t('ops.exitCode') }} {{ result.exitCode ?? '—' }} ·
              {{ formatDuration(result.durationMs) }}
            </span>
          </div>

          <div v-if="result.stdout" class="ops-output">
            <div class="hint">stdout</div>
            <pre class="diff">{{ result.stdout }}</pre>
          </div>
          <div v-if="result.stderr" class="ops-output">
            <div class="hint">stderr</div>
            <pre class="diff danger-text">{{ result.stderr }}</pre>
          </div>
          <p class="hint" style="margin: 0">{{ t('ops.refreshHint') }}</p>

          <!-- 日志路径：命令行窗口里的完整输出也在这里，方便事后回看 -->
          <div v-if="result.logPath" class="ops-log">
            <span class="hint">{{ t('ops.logPath') }}</span>
            <code class="mono">{{ result.logPath }}</code>
            <button class="btn btn--sm" @click="copyLogPath">{{ t('browse.copy') }}</button>
          </div>
        </template>
      </div>

      <div class="modal__foot">
        <button class="btn" @click="close">
          {{ result ? t('clean.close') : t('registry.cancel') }}
        </button>
        <button
          v-if="!result"
          class="btn"
          :class="requiresAck ? 'btn--danger' : 'btn--primary'"
          :disabled="!canRun || running"
          @click="run"
        >
          {{ running ? t('ops.running') : t('ops.run') }}
        </button>
      </div>
    </div>
  </div>
</template>
