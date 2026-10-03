<script setup lang="ts">
/**
 * 包管理总览页（应用默认落地页）。
 *
 * 一个管理器一张卡片：
 * - 已检测到 → 显示 logo / 版本 / 全局目录 / 缓存目录 / 包数量 + 「管理此管理器」
 * - 未检测到 → 显示「未检测到」+ 「前往官网下载」
 *
 * 不再按「一期/二期/三期」分组，也不再显示阶段标签 —— 探测到就能用，没探测到就给下载入口。
 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagerInfo } from '@/types'
import ManagerLogo from '@/components/ManagerLogo.vue'
import { formatBytes, formatCount, ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const managers = computed(() => store.sidebarManagers)
const installedCount = computed(() => store.installed.length)

function packageCount(managerId: string) {
  return store.packageCountOf(managerId)
}

function cacheBytes(managerId: string) {
  return store.cacheOf(managerId)
}

/** 点击卡片 → 进入该管理器的详情页（概览分页） */
function open(manager: ManagerInfo) {
  store.openManager(manager.id)
}

/** 未安装时只提供下载入口，不进入详情 */
function download(manager: ManagerInfo) {
  void store.openManagedLink('manager', manager.id)
}
</script>

<template>
  <section class="scroll-area">
    <div class="col">
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ t('manage.title') }}</span>
          <span class="panel__sub">{{ t('manage.subtitle') }}</span>
          <span class="panel__spacer" />
          <span class="tag tag--ok">{{ installedCount }} / {{ store.managers.length }}</span>
        </div>
      </div>

      <div v-if="store.managers.length === 0" class="empty">
        <div class="empty__icon">📦</div>
        <div>{{ t('manage.emptyTitle') }}</div>
        <div class="hint">{{ t('manage.emptyHint') }}</div>
      </div>

      <div v-else class="manager-grid">
        <article
          v-for="manager in managers"
          :key="manager.id"
          class="manager-card"
          :class="{ 'is-missing': !manager.detected }"
        >
          <header class="manager-card__head">
            <ManagerLogo :manager-id="manager.id" :name="manager.name" size="sm" />
            <span class="manager-card__name">{{ manager.name }}</span>
            <span class="tag">{{ manager.language }}</span>
            <span class="panel__spacer" />
            <span class="dot" :class="manager.detected ? 'dot--ok' : 'dot--missing'" />
          </header>

          <div class="manager-card__body">
            <template v-if="manager.detected">
              <div class="kv">
                <span class="kv__k">{{ t('manage.version') }}</span>
                <span class="kv__v">{{ manager.version ?? '—' }}</span>
                <span class="kv__k">{{ t('manage.globalRoot') }}</span>
                <span class="kv__v" :title="manager.globalRoot ?? ''">
                  {{ ellipsisPath(manager.globalRoot, 42) }}
                </span>
                <span class="kv__k">{{ t('manage.cacheDir') }}</span>
                <span class="kv__v" :title="manager.cacheDir ?? ''">
                  {{ ellipsisPath(manager.cacheDir, 42) }}
                </span>
              </div>

              <div class="manager-card__stats">
                <span class="tag tag--accent">
                  {{ t('manage.packageCount', { count: formatCount(packageCount(manager.id)) }) }}
                </span>
                <span v-if="cacheBytes(manager.id) !== null" class="tag">
                  {{ t('manage.cacheSize', { size: formatBytes(cacheBytes(manager.id)) }) }}
                </span>
              </div>
            </template>

            <template v-else>
              <div class="manager-card__missing">
                <span>{{ t('manage.missing') }}</span>
                <span class="hint">{{ t('manage.notDetectedHint') }}</span>
              </div>
              <div v-if="manager.warnings.length" class="hint">{{ manager.warnings[0] }}</div>
            </template>
          </div>

          <footer class="manager-card__foot">
            <template v-if="manager.detected">
              <button class="btn btn--primary btn--sm" @click="open(manager)">
                {{ t('manage.manage') }}
              </button>
            </template>
            <template v-else>
              <button
                class="btn btn--primary btn--sm"
                :disabled="!manager.downloadUrl"
                @click="download(manager)"
              >
                {{ t('manage.download') }}
              </button>
              <button
                class="btn btn--sm"
                :disabled="!manager.docsUrl"
                @click="store.openManagedLink('docs', manager.id)"
              >
                {{ t('manage.docs') }}
              </button>
            </template>
          </footer>
        </article>
      </div>

      <p class="hint">
        「前往官网下载」只会用系统浏览器打开官方页面，不会自动安装任何东西。
      </p>
    </div>
  </section>
</template>
