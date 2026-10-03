<script setup lang="ts">
/**
 * 浏览 / 安装新包（管理器详情页的「浏览 / 安装」分页）。
 *
 * 行为边界（刻意如此）：
 * - 只在各生态**官方搜索 API** 里查询，关键词由后端校验与 URL 编码；
 * - 结果显示热度、版本与包主页；
 * - 「安装方式」只生成命令并提供一键复制 —— **不会替你执行**。
 *   安装会改动真实环境，且依赖解析、权限确认、交互提示都无法在后台可靠完成。
 */
import { computed, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagerInfo, RemotePackage } from '@/types'
import { formatCount, matchesKeyword } from '@/utils/format'

const props = defineProps<{ manager: ManagerInfo }>()

const store = useAppStore()
const { t } = useI18n()

const query = ref('')
const copied = ref<string | null>(null)

const results = computed(() => store.remotePackages)
const plan = computed(() => store.installPlan)

async function run() {
  if (!query.value.trim()) return
  await store.browse(query.value)
}

/** 点某个结果 → 生成安装方案（只给命令） */
async function choose(item: RemotePackage) {
  await store.planInstall(item.name)
}

/** 本机已安装的包（按当前管理器过滤一次，供下面的查表用） */
const installed = computed(() =>
  (store.report?.packages ?? []).filter((record) => record.manager === props.manager.id),
)

/**
 * 该包在本机是否已经安装？返回已安装版本，未安装返回 null。
 *
 * 为什么需要：用户常在这里搜一个**其实已经装过**的包，然后以为「搜不到」——
 * 在线浏览只查仓库、不查本机，必须显式提示，否则这就是个体验坑。
 * 匹配复用全局搜索的宽容规则（忽略空格/连字符/点），
 * 因此仓库里的 `JanDeDobbeleer.OhMyPosh` 能对上本机记录的包名。
 */
function installedVersion(item: RemotePackage): string | null {
  const hit = installed.value.find(
    (record) => matchesKeyword([record.name, record.description], item.name),
  )
  return hit ? hit.version ?? '' : null
}

async function copyCommand(command: string) {
  try {
    await navigator.clipboard.writeText(command)
    copied.value = command
    store.notify('info', t('browse.copied'))
    window.setTimeout(() => {
      if (copied.value === command) copied.value = null
    }, 2500)
  } catch {
    store.notify('warn', t('browse.copyFailed'))
  }
}

function planFor(item: RemotePackage): string {
  return item.installCommand ?? `${props.manager.id} install ${item.name}`
}
</script>

<template>
  <div class="col">
    <div class="panel">
      <div class="panel__head">
        <span class="panel__title">{{ t('browse.title', { name: manager.name }) }}</span>
        <span class="panel__spacer" />
        <span v-if="results.length" class="tag">{{ t('browse.results', { count: results.length }) }}</span>
      </div>

      <div class="panel__body col">
        <div class="row">
          <input
            v-model="query"
            class="input"
            style="flex: 1"
            type="search"
            :placeholder="t('browse.placeholder')"
            @keydown.enter="run"
          />
          <button class="btn btn--primary" :disabled="store.browseLoading || !query.trim()" @click="run">
            {{ store.browseLoading ? t('browse.searching') : t('browse.search') }}
          </button>
        </div>

        <div class="banner banner--info">{{ t('browse.noExecute') }}</div>

        <!-- 能力限制提示（例如 PyPI 只支持精确名查询）与失败原因：分开显示 -->
        <div v-if="store.browseHint" class="banner banner--warn">{{ store.browseHint }}</div>
        <div v-if="store.browseError" class="banner banner--error">{{ store.browseError }}</div>
        <div v-else-if="store.browseNote" class="banner banner--error">{{ store.browseNote }}</div>

        <!-- 安装方案：只给命令 -->
        <div v-if="plan" class="install-plan">
          <div class="install-plan__head">
            <span class="tag tag--accent">{{ t('browse.install') }}</span>
            <code class="install-plan__cmd">{{ plan.command }}</code>
            <span class="panel__spacer" />
            <span v-if="plan.requiresAdmin" class="tag tag--warn">{{ t('browse.adminRequired') }}</span>
            <button class="btn btn--sm" @click="copyCommand(plan.command)">
              {{ copied === plan.command ? '✓' : t('browse.copy') }}
            </button>
            <button class="btn btn--ghost btn--sm" @click="store.clearInstallPlan()">✕</button>
          </div>
          <div class="hint">{{ plan.explanation }}</div>
        </div>

        <!-- 空结果：三种情况分别说明，不要一律说「没找到」 -->
        <div
          v-if="
            !store.browseLoading &&
            results.length === 0 &&
            !store.browseError &&
            !store.browseNote
          "
          class="empty"
          style="padding: 34px"
        >
          <div class="empty__icon">🔍</div>
          <div v-if="!store.browseQuery">{{ t('browse.initial') }}</div>
          <div v-else-if="store.browseUnsupported">{{ t('browse.unsupported') }}</div>
          <div v-else>{{ t('browse.empty') }}</div>
          <div v-if="store.browseQuery && !store.browseUnsupported" class="hint">
            {{ t('browse.emptyHint') }}
          </div>
        </div>

        <ul v-if="results.length > 0" class="remote-list list-reset">
          <li v-for="item in results" :key="`${item.name}@${item.version ?? ''}`" class="remote-item">
            <div class="remote-item__main">
              <div class="remote-item__title">
                <span class="remote-item__name mono">{{ item.name }}</span>
                <span v-if="item.version" class="tag">{{ item.version }}</span>
                <span v-if="item.downloads" class="tag">
                  {{ t('browse.downloads') }} {{ formatCount(item.downloads) }}
                </span>
                <span v-if="installedVersion(item) !== null" class="tag tag--ok">
                  {{ t('browse.alreadyInstalled') }}
                  <template v-if="installedVersion(item)"> {{ installedVersion(item) }}</template>
                </span>
              </div>
              <div v-if="item.description" class="remote-item__desc">{{ item.description }}</div>
              <code class="remote-item__cmd">{{ planFor(item) }}</code>
            </div>
            <div class="remote-item__actions">
              <button
                class="btn btn--ghost btn--sm"
                :disabled="!item.homepage"
                @click="item.homepage && store.openUrl(item.homepage)"
              >
                {{ t('browse.homepage') }}
              </button>
              <button class="btn btn--sm" @click="copyCommand(planFor(item))">
                {{ copied === planFor(item) ? '✓' : t('browse.copy') }}
              </button>
              <button
                class="btn btn--sm"
                :class="installedVersion(item) === null ? 'btn--primary' : ''"
                :title="installedVersion(item) !== null ? t('browse.reinstallHint') : ''"
                @click="choose(item)"
              >
                {{ installedVersion(item) !== null ? t('browse.reinstall') : t('browse.install') }}
              </button>
            </div>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
