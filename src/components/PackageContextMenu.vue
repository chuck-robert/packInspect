<script setup lang="ts">
/**
 * 包右键菜单。
 *
 * 交互：在包列表行上右键 → 弹出菜单 → 选择动作。
 * - 「管理此包」/「查看安装详情」/「打开包主页」是真能用的（一期）
 * - 更新 / 卸载 / 重新安装 为**占位按钮**：灰色、标 `占位`，hover 时显示等价官方命令
 *   这样既满足「右键可管理」的交互，又不会误点破坏用户环境。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagementAction, PackageRecord } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'

const props = defineProps<{
  record: PackageRecord
  x: number
  y: number
}>()

const emit = defineEmits<{
  close: []
  /** 打开包详情抽屉 */
  inspect: [PackageRecord]
  /** 展开包内插件树 */
  manage: [PackageRecord]
  /** 请求执行真实操作（更新 / 卸载 / 安装），由父组件弹确认框 */
  operate: [PackageRecord, ManagementAction]
}>()

const store = useAppStore()
const { t } = useI18n()
const actions = ref<ManagementAction[]>([])
const loading = ref(true)
const hovered = ref<string | null>(null)

/** 菜单尺寸用于边界收敛，避免贴到屏幕外 */
const MENU_WIDTH = 300
const MENU_MAX_HEIGHT = 420

const position = computed(() => {
  const maxX = window.innerWidth - MENU_WIDTH - 8
  const maxY = window.innerHeight - MENU_MAX_HEIGHT - 8
  return {
    left: `${Math.max(8, Math.min(props.x, maxX))}px`,
    top: `${Math.max(8, Math.min(props.y, maxY))}px`,
  }
})

const hoveredAction = computed(() => actions.value.find((a) => a.action === hovered.value) ?? null)

function actionLabel(action: ManagementAction) {
  const map: Record<string, string> = {
    manage: 'menu.manage',
    inspect: 'menu.inspect',
    update: 'menu.update',
    uninstall: 'menu.uninstall',
    install: 'menu.install',
    disable: 'menu.disable',
    openDocs: 'menu.openDocs',
  }
  return map[action.action] ? t(map[action.action]) : action.label
}

async function run(action: ManagementAction) {
  if (!action.enabled) return
  switch (action.action) {
    case 'manage':
      emit('manage', props.record)
      emit('close')
      break
    case 'inspect':
      emit('inspect', props.record)
      emit('close')
      break
    case 'openDocs':
      if (action.note) await store.openUrl(action.note)
      emit('close')
      break
    case 'update':
    case 'uninstall':
    case 'install':
      // 真实执行交给父组件的确认对话框：本组件只负责发起意图
      emit('operate', props.record, action)
      emit('close')
      break
    default:
      break
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') emit('close')
}

onMounted(async () => {
  actions.value = await store.loadActions(props.record)
  loading.value = false
  window.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <!-- 透明遮罩：点击任意位置关闭菜单 -->
  <div class="ctx-backdrop" @click="emit('close')" @contextmenu.prevent="emit('close')">
    <div class="ctx-menu" :style="position" @click.stop>
      <!-- 头部：包身份 -->
      <div class="ctx-head">
        <ManagerLogo :manager-id="record.manager" :name="record.manager" size="sm" />
        <div class="ctx-head__text">
          <div class="ctx-head__name">{{ record.name }}</div>
          <div class="ctx-head__meta mono">
            <span class="tag">{{ record.manager }}</span>
            <span>{{ record.version ?? '—' }}</span>
            <span class="tag" :class="record.scope === 'global' ? 'tag--accent' : ''">
              {{ t(`scope.${record.scope}`) }}
            </span>
          </div>
        </div>
      </div>

      <div v-if="loading" class="ctx-loading">
        <span class="spinner" /> <span>{{ t('menu.actions') }}…</span>
      </div>

      <ul v-else class="ctx-list list-reset">
        <li v-for="action in actions" :key="action.action">
          <button
            class="ctx-item"
            :class="{
              'is-disabled': !action.enabled,
              'is-destructive': action.destructive,
            }"
            :disabled="!action.enabled"
            @mouseenter="hovered = action.action"
            @mouseleave="hovered = null"
            @click="run(action)"
          >
            <span class="ctx-item__label">{{ actionLabel(action) }}</span>
            <span v-if="action.online" class="tag" title="需要联网">net</span>
            <span v-if="action.destructive" class="tag tag--danger">!</span>
            <span v-if="!action.enabled" class="tag tag--warn">{{ t('menu.placeholder') }}</span>
          </button>
        </li>
      </ul>

      <!-- 占位说明 / 等价命令：hover 时展示，避免菜单本身过于拥挤 -->
      <div v-if="hoveredAction && !hoveredAction.enabled" class="ctx-foot">
        <div class="ctx-foot__note">{{ hoveredAction.note }}</div>
        <div v-if="hoveredAction.commandHint" class="ctx-foot__cmd mono">
          {{ t('menu.command') }}:
          <code>{{ hoveredAction.commandHint }}</code>
        </div>
      </div>
      <div v-else-if="hoveredAction?.note && hoveredAction.enabled" class="ctx-foot">
        <div class="ctx-foot__note">{{ hoveredAction.note }}</div>
      </div>
    </div>
  </div>
</template>
