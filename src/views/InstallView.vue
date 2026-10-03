<script setup lang="ts">
/**
 * 安装界面（独立视图）。
 *
 * 与「管理器详情 → 浏览/安装」分页的区别：
 * - 这一页是**任务导向**的 —— 目标是「装一个包」，因此左侧直接列出可用仓库，
 *   选完就能搜、就能装，不需要先理解包管理器的概念
 * - 详情页的分页是**对象导向**的 —— 已经知道自己要管理哪个管理器
 *
 * 为什么不做成模态框：安装是有后续动作的（看结果、看日志、装下一个），
 * 模态框会一直挡着内容；而且搜索、结果列表、执行结果三块信息量放不进一个框。
 *
 * 安全：这一页不新增执行路径 —— 「安装」走的仍是唯一的 `run_package_op`
 * （经 PackageOpDialog 确认，静态模板 + 超时 + 不走 shell）。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type {
  ManagementAction,
  ManagerInfo,
  PackageOpResult,
  PackageRecord,
  RemotePackage,
} from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'
import PackageOpDialog from '@/components/PackageOpDialog.vue'
import { formatCount, ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const query = ref('')
const copied = ref<string | null>(null)

/** 选中的仓库（包管理器） */
const selectedId = ref<string | null>(null)

/**
 * 可用于安装的仓库：必须**平台适用**且**已检测到**。
 * 没装的管理器自己都跑不起来，更不可能替你装东西。
 */
const installable = computed<ManagerInfo[]>(() =>
  store.managers
    .filter((m) => m.platformApplicable && m.detected)
    .sort((a, b) => a.name.localeCompare(b.name)),
)

const selected = computed<ManagerInfo | null>(
  () => installable.value.find((m) => m.id === selectedId.value) ?? null,
)

const results = computed(() => store.remotePackages)
const plan = computed(() => store.installPlan)

/** 切换仓库：清掉上一个仓库的搜索结果与方案，并同步到全局 activeManager */
async function select(id: string) {
  selectedId.value = id
  query.value = ''
  store.resetBrowse()
  store.setActiveManager(id)
}

watch(selectedId, () => {
  if (selectedId.value) store.setActiveManager(selectedId.value)
})

onMounted(() => {
  // 默认选中第一个可用仓库；若全局已选了某个管理器则优先沿用
  const preferred =
    store.activeManager && installable.value.some((m) => m.id === store.activeManager)
      ? store.activeManager
      : installable.value[0]?.id ?? null
  if (preferred) void select(preferred)
})

async function run() {
  const q = query.value.trim()
  if (!q || !selectedId.value) return
  // browse 依赖 store.activeManager，这里确保一致
  store.setActiveManager(selectedId.value)
  await store.browse(q)
}

/** 点某个结果 → 生成安装方案（含确切命令），真正执行要再确认 */
async function choose(item: RemotePackage) {
  store.setActiveManager(selectedId.value!)
  await store.planInstall(item.name)
}

// ---- 执行确认 ----
const execOpen = ref(false)
const execRecord = ref<PackageRecord | null>(null)
const execAction = ref<ManagementAction | null>(null)

function requestExecute() {
  const current = plan.value
  if (!current) return
  execRecord.value = {
    manager: current.managerId,
    name: current.package,
    version: null,
    scope: 'global',
    path: null,
    size: null,
    redundant: false,
    redundantReason: null,
    description: null,
    latestVersion: null,
    plugins: [],
    pluginsLoaded: false,
  }
  execAction.value = {
    action: 'install',
    label: t('browse.install'),
    online: true,
    destructive: false,
    enabled: true,
    commandHint: current.command,
    note: current.explanation,
  }
  execOpen.value = true
}

const lastResult = ref<PackageOpResult | null>(null)

/** 执行结束后重扫该管理器、清掉方案，并记下结果用于页面内回显 */
async function onExecuted() {
  lastResult.value = store.lastOpResult
  const id = selectedId.value
  if (id) await store.scan({ managers: [id], silent: true })
  store.clearInstallPlan()
}

async function copyCommand(command: string) {
  try {
    await navigator.clipboard.writeText(command)
    copied.value = command
    store.notify('info', t('browse.copied'))
    window.setTimeout(() => {
      if (copied.value === command) copied.value = null
    }, 2000)
  } catch {
    store.notify('warn', t('browse.copyFailed'))
  }
}

/** 该包在本机是否已安装（用与全局搜索一致的宽容匹配） */
function installedVersion(item: RemotePackage): string | null {
  const hit = (store.report?.packages ?? []).find((r) => r.name.toLowerCase() === item.name.toLowerCase())
  return hit ? hit.version ?? '' : null
}
</script>

<template>
  <section class="scroll-area" style="padding: 12px 14px">
    <div class="install-layout">
      <!-- 左：可选仓库 -->
      <div class="panel install-repos">
        <div class="panel__head">
          <span class="panel__title">{{ t('install.repo') }}</span>
          <span class="panel__spacer" />
          <span class="tag">{{ installable.length }}</span>
        </div>
        <div class="install-repos__body">
          <button
            v-for="manager in installable"
            :key="manager.id"
            class="install-repo"
            :class="{ 'is-active': manager.id === selectedId }"
            @click="select(manager.id)"
          >
            <ManagerLogo :manager-id="manager.id" :name="manager.name" size="sm" />
            <span class="install-repo__name">{{ manager.name }}</span>
            <span class="install-repo__lang">{{ manager.language }}</span>
          </button>

          <div v-if="installable.length === 0" class="hint" style="padding: 12px">
            {{ t('install.noRepo') }}
          </div>
        </div>
      </div>

      <!-- 右：搜索 + 结果 + 方案 -->
      <div class="col" style="min-width: 0; flex: 1">
        <!-- 搜索区 -->
        <div class="panel">
          <div class="panel__head">
            <ManagerLogo
              v-if="selected"
              :manager-id="selected.id"
              :name="selected.name"
              size="sm"
            />
            <span class="panel__title">{{ t('install.searchTitle') }}</span>
            <span v-if="selected" class="tag">{{ selected.name }}</span>
          </div>
          <div class="panel__body col">
            <div class="row">
              <input
                v-model="query"
                class="input"
                style="flex: 1"
                :placeholder="t('install.placeholder', { manager: selected?.name ?? '' })"
                spellcheck="false"
                @keydown.enter="run"
              />
              <button
                class="btn btn--primary"
                :disabled="store.browseLoading || !query.trim() || !selectedId"
                @click="run"
              >
                {{ store.browseLoading ? t('browse.searching') : t('browse.search') }}
              </button>
            </div>

            <div v-if="store.browseHint" class="banner banner--warn">{{ store.browseHint }}</div>
            <div v-if="store.browseError" class="banner banner--error">
              {{ store.browseError }}
            </div>
            <div v-else-if="store.browseNote" class="banner banner--error">
              {{ store.browseNote }}
            </div>
          </div>
        </div>

        <!-- 安装方案 -->
        <div v-if="plan" class="panel">
          <div class="panel__head">
            <span class="panel__title">{{ t('install.plan') }}</span>
            <span class="panel__spacer" />
            <span v-if="plan.requiresAdmin" class="tag tag--warn">
              {{ t('browse.adminRequired') }}
            </span>
          </div>
          <div class="panel__body col">
            <div class="ops-command">
              <code>{{ plan.command }}</code>
              <button class="btn btn--sm" @click="copyCommand(plan.command)">
                {{ copied === plan.command ? '✓' : t('browse.copy') }}
              </button>
            </div>
            <div class="hint">{{ plan.explanation }}</div>
            <div class="row" style="justify-content: flex-end">
              <button class="btn" @click="store.clearInstallPlan()">
                {{ t('registry.cancel') }}
              </button>
              <button class="btn btn--primary" @click="requestExecute">
                {{ t('ops.run') }}
              </button>
            </div>
          </div>
        </div>

        <!-- 上次执行结果 -->
        <div v-if="lastResult" class="panel">
          <div class="panel__head">
            <span class="panel__title">{{ t('install.lastResult') }}</span>
            <span class="panel__spacer" />
            <span class="tag" :class="lastResult.success ? 'tag--ok' : 'tag--danger'">
              {{ lastResult.success ? t('install.succeeded') : t('install.failed') }}
            </span>
            <button class="btn btn--ghost btn--sm" @click="lastResult = null">✕</button>
          </div>
          <div class="panel__body col">
            <div class="hint mono">{{ lastResult.command }}</div>
            <pre v-if="lastResult.stdout" class="diff">{{ lastResult.stdout }}</pre>
            <pre v-if="lastResult.stderr" class="diff danger-text">{{ lastResult.stderr }}</pre>
            <div v-if="lastResult.logPath" class="ops-log">
              <span class="hint">{{ t('ops.logPath') }}</span>
              <code class="mono">{{ ellipsisPath(lastResult.logPath, 70) }}</code>
            </div>
          </div>
        </div>

        <!-- 搜索结果 -->
        <div class="panel">
          <div class="panel__head">
            <span class="panel__title">{{ t('install.results') }}</span>
            <span class="panel__spacer" />
            <span v-if="results.length" class="tag">{{ formatCount(results.length) }}</span>
          </div>

          <div v-if="store.browseLoading" class="empty" style="padding: 30px">
            <span class="spinner" />
            <div>{{ t('browse.searching') }}</div>
          </div>

          <div
            v-else-if="results.length === 0"
            class="empty"
            style="padding: 34px"
          >
            <div class="empty__icon">📦</div>
            <div v-if="!store.browseQuery">{{ t('install.prompt') }}</div>
            <div v-else-if="store.browseUnsupported">{{ t('browse.unsupported') }}</div>
            <div v-else>{{ t('browse.empty') }}</div>
            <div v-if="store.browseQuery && !store.browseUnsupported" class="hint">
              {{ t('browse.emptyHint') }}
            </div>
          </div>

          <ul v-else class="remote-list list-reset">
            <li
              v-for="item in results"
              :key="`${item.name}@${item.version ?? ''}`"
              class="remote-item"
            >
              <div class="remote-item__main">
                <div class="remote-item__title">
                  <span class="remote-item__name mono">{{ item.name }}</span>
                  <span v-if="item.version" class="tag">{{ item.version }}</span>
                  <span v-if="item.downloads" class="tag">
                    {{ t('browse.downloads') }} {{ formatCount(item.downloads) }}
                  </span>
                  <span v-if="installedVersion(item) !== null" class="tag tag--ok">
                    {{ t('browse.alreadyInstalled') }}
                  </span>
                </div>
                <div v-if="item.description" class="remote-item__desc">
                  {{ item.description }}
                </div>
              </div>
              <div class="remote-item__actions">
                <button
                  v-if="item.homepage"
                  class="btn btn--ghost btn--sm"
                  :title="item.homepage"
                  @click="store.openUrl(item.homepage!)"
                >
                  {{ t('browse.homepage') }}
                </button>
                <button class="btn btn--sm" @click="choose(item)">
                  {{ t('install.planAction') }}
                </button>
              </div>
            </li>
          </ul>
        </div>
      </div>
    </div>

    <PackageOpDialog
      v-model:open="execOpen"
      :record="execRecord"
      :action="execAction"
      @done="onExecuted"
    />
  </section>
</template>
