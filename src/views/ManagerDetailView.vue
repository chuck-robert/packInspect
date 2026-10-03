<script setup lang="ts">
/**
 * 包管理器详情页。
 *
 * 这是点击左侧某个包管理器后的落地视图：**默认先看概览**（它是什么、装在哪、有多少包），
 * 再用分页进入「包列表 / 浏览安装 / 管理操作」。
 * 未安装时整页变成下载引导 —— 没有可执行文件就读不到包列表，硬展示空表没有意义。
 */
import { computed, onMounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagementAction, ManagerTab, PackageRecord } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'
import ManagerPackagesPanel from '@/components/ManagerPackagesPanel.vue'
import ManagerBrowsePanel from '@/components/ManagerBrowsePanel.vue'
import PackageOpDialog from '@/components/PackageOpDialog.vue'
import { formatBytes, formatCount, ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()
const expanded = ref(false)
/** 待执行的真实操作（右键菜单发起，交给 PackageOpDialog 确认） */
const opState = ref<{ record: PackageRecord; action: ManagementAction } | null>(null)
const opOpen = ref(false)

function requestOp(record: PackageRecord, action: ManagementAction) {
  opState.value = { record, action }
  opOpen.value = true
}

const manager = computed(() => store.activeManagerInfo)

const tabs = computed<{ key: ManagerTab; labelKey: string }[]>(() => [
  { key: 'overview', labelKey: 'manager.overview' },
  { key: 'packages', labelKey: 'manager.packages' },
  { key: 'browse', labelKey: 'manager.browse' },
  { key: 'manage', labelKey: 'manager.manage' },
])

const packageCount = computed(() =>
  manager.value ? store.packageCountOf(manager.value.id) : 0,
)
const cacheBytes = computed(() =>
  manager.value ? store.cacheOf(manager.value.id) : null,
)
/** 已扫过这个管理器吗（用于「扫描」按钮的文案与状态） */
const scanned = computed(() => manager.value ? store.hasScanned(manager.value.id) : false)

/** 未安装时的推荐安装方式 */
const installHint = computed(() =>
  manager.value ? store.hints.find((h) => h.managerId === manager.value!.id)?.installHint ?? null : null,
)

/** 切换到缓存视图时需要先设置过滤范围 */
function openCache() {
  const id = manager.value?.id
  if (!id) return
  store.setActiveManager(id)
  store.setView('cache')
}

/** 切换到镜像源视图 */
function openRegistry() {
  const id = manager.value?.id
  if (!id) return
  store.setActiveManager(id)
  store.setView('registry')
}

async function scanThis() {
  if (!manager.value) return
  await store.scan({ managers: [manager.value.id], measureSize: false })
}

onMounted(() => {
  // 直接进入详情页时若还没扫过，自动扫一次，让概览的数字有意义
  if (manager.value?.detected && !scanned.value && !store.scanning) {
    void scanThis()
  }
})
</script>

<template>
  <section v-if="!manager" class="scroll-area">
    <div class="empty">
      <div class="empty__icon">📦</div>
      <div>{{ t('nav.managersSection') }}</div>
      <div class="hint">{{ t('search.hint') }}</div>
    </div>
  </section>

  <section v-else class="scroll-area">
    <div class="col">
      <!-- 头部：logo + 名称 + 版本 -->
      <div class="panel">
        <div class="panel__head manager-header">
          <ManagerLogo :manager-id="manager.id" :name="manager.name" size="lg" />
          <div class="manager-header__text">
            <div class="manager-header__name">
              {{ manager.name }}
              <span v-if="manager.detected" class="tag tag--ok">
                {{ manager.version ?? t('manage.installed') }}
              </span>
              <span v-else class="tag tag--danger">{{ t('manage.missing') }}</span>
            </div>
            <div class="manager-header__sub mono" :title="manager.exePath ?? ''">
              {{ manager.exePath ?? t('manage.notDetectedHint') }}
            </div>
          </div>
          <span class="panel__spacer" />
          <button
            v-if="manager.detected"
            class="btn btn--primary btn--sm"
            :disabled="store.scanning"
            @click="scanThis"
          >
            {{ store.scanning ? t('toolbar.scanning') : scanned ? t('manager.rescan') : t('manager.scanNow') }}
          </button>
        </div>

        <!-- 分页 -->
        <nav class="tabs">
          <button
            v-for="tab in tabs"
            :key="tab.key"
            class="tabs__item"
            :class="{ 'is-active': store.managerTab === tab.key }"
            @click="store.setManagerTab(tab.key)"
          >
            {{ t(tab.labelKey) }}
            <span v-if="tab.key === 'packages' && packageCount" class="tabs__badge">
              {{ formatCount(packageCount) }}
            </span>
          </button>
        </nav>
      </div>

      <!-- 未安装：整页下载引导 -->
      <template v-if="!manager.detected">
        <div class="panel">
          <div class="panel__body col">
            <div class="banner banner--warn">
              {{ t('manager.notInstalledTitle', { name: manager.name }) }}
            </div>
            <p class="hint" style="margin: 0">{{ t('manager.notInstalledBody') }}</p>

            <div v-if="manager.warnings.length" class="hint">
              {{ t('manager.warnings') }}：{{ manager.warnings.join('；') }}
            </div>

            <div v-if="installHint" class="kv">
              <span class="kv__k">{{ t('manager.installHint') }}</span>
              <span class="kv__v">{{ installHint }}</span>
            </div>

            <div class="row">
              <button
                class="btn btn--primary"
                :disabled="!manager.downloadUrl"
                @click="store.openManagedLink('manager', manager.id)"
              >
                {{ t('manager.download') }}
              </button>
              <button
                class="btn"
                :disabled="!manager.docsUrl"
                @click="store.openManagedLink('docs', manager.id)"
              >
                {{ t('manager.docs') }}
              </button>
            </div>
          </div>
        </div>
      </template>

      <!-- 已安装：按分页展示 -->
      <template v-else>
        <!-- 概览 -->
        <template v-if="store.managerTab === 'overview'">
          <div class="stat-grid">
            <div class="stat">
              <div class="stat__label">{{ t('manager.packageCount') }}</div>
              <div class="stat__value">{{ formatCount(packageCount) }}</div>
            </div>
            <div class="stat">
              <div class="stat__label">{{ t('manager.cacheSize') }}</div>
              <div class="stat__value">{{ cacheBytes === null ? '—' : formatBytes(cacheBytes) }}</div>
            </div>
            <div class="stat">
              <div class="stat__label">{{ t('manager.language') }}</div>
              <div class="stat__value" style="font-size: 13px">{{ manager.language }}</div>
            </div>
          </div>

          <div class="panel">
            <div class="panel__head">
              <span class="panel__title">{{ t('manager.paths') }}</span>
              <span class="panel__spacer" />
              <button class="btn btn--ghost btn--sm" @click="expanded = !expanded">
                {{ expanded ? '−' : '+' }}
              </button>
            </div>
            <div class="panel__body">
              <div class="kv">
                <span class="kv__k">{{ t('manager.exePath') }}</span>
                <span class="kv__v">{{ manager.exePath ?? '—' }}</span>
                <span class="kv__k">{{ t('manager.globalRoot') }}</span>
                <span class="kv__v">{{ manager.globalRoot ?? '—' }}</span>
                <span class="kv__k">{{ t('manager.cacheDir') }}</span>
                <span class="kv__v">{{ manager.cacheDir ?? '—' }}</span>
                <template v-if="expanded">
                  <span class="kv__k">{{ t('manager.configFile') }}</span>
                  <span class="kv__v">{{ manager.configFile ?? '—' }}</span>
                  <span class="kv__k">{{ t('manager.version') }}</span>
                  <span class="kv__v">{{ manager.version ?? '—' }}</span>
                </template>
              </div>
            </div>
          </div>

          <!-- 快捷入口 -->
          <div class="panel">
            <div class="panel__head">
              <span class="panel__title">{{ t('toolbar.packages') }}</span>
            </div>
            <div class="panel__body row" style="flex-wrap: wrap">
              <button class="btn" @click="store.setManagerTab('packages')">
                {{ t('manager.openPackages') }}
              </button>
              <button class="btn" @click="store.setManagerTab('browse')">
                {{ t('manager.browse') }}
              </button>
              <button class="btn" @click="openCache">{{ t('manager.openCache') }}</button>
            </div>
          </div>
        </template>

        <!-- 包列表 -->
        <ManagerPackagesPanel v-else-if="store.managerTab === 'packages'" :manager="manager" @operate="requestOp" />

        <!-- 浏览 / 安装 -->
        <ManagerBrowsePanel v-else-if="store.managerTab === 'browse'" :manager="manager" />

        <!-- 管理操作 -->
        <template v-else>
          <div class="panel">
            <div class="panel__head">
              <span class="panel__title">{{ t('manager.manage') }}</span>
            </div>
            <div class="panel__body col">
              <div class="banner banner--info">{{ t('browse.noExecute') }}</div>

              <div class="kv">
                <span class="kv__k">{{ t('registry.key') }}</span>
                <span class="kv__v">
                  {{ store.activeRegistry ? '' : t('manager.noRegistry') }}
                </span>
              </div>

              <div class="row" style="flex-wrap: wrap">
                <button class="btn" @click="openRegistry">{{ t('manager.openRegistry') }}</button>
                <button class="btn" :disabled="!manager.docsUrl" @click="store.openManagedLink('docs', manager.id)">
                  {{ t('manager.docs') }}
                </button>
                <button
                  class="btn"
                  :disabled="!manager.downloadUrl"
                  @click="store.openManagedLink('manager', manager.id)"
                >
                  {{ t('manager.download') }}
                </button>
              </div>

              <div v-if="manager.configFile" class="hint">
                {{ t('manager.configFile') }}：{{ ellipsisPath(manager.configFile, 90) }}
              </div>
            </div>
          </div>
        </template>
      </template>
    </div>

    <PackageOpDialog
      v-model:open="opOpen"
      :record="opState?.record ?? null"
      :action="opState?.action ?? null"
    />
  </section>
</template>
