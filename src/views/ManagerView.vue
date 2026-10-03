<script setup lang="ts">
/**
 * 包管理页（默认落地页）。
 *
 * 一个管理器一张卡片：
 * - 已检测到 → 显示版本 / 全局目录 / 缓存目录 / 已扫到的包数量 + 「管理此管理器」
 * - 未检测到 → 显示「未检测到」+ 「前往官网下载」按钮（满足需求 2）
 *
 * 三期规划里的管理器也列出来，但标为「后续支持」并弱化显示，避免用户以为坏了。
 */
import { computed, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import type { ManagerInfo } from '@/types'
import { formatBytes, formatCount, ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()
const expanded = ref<Set<string>>(new Set())

const tiers = computed(() => store.byTier)
const installedCount = computed(() => store.installed.length)

function tierTitle(tier: number) {
  if (tier === 1) return t('manage.tier1')
  if (tier === 2) return t('manage.tier2')
  return t('manage.tier3')
}

function toggleDetail(id: string) {
  const next = new Set(expanded.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expanded.value = next
}

/** 点「管理此管理器」：切到包列表并只看这个管理器；没数据就先扫一次 */
async function manage(manager: ManagerInfo) {
  store.setActiveManager(manager.id)
  store.setView('packages')
  if (!store.hasScanned(manager.id)) {
    await store.scan({ managers: [manager.id], measureSize: false })
  }
}

function packageCount(managerId: string) {
  return store.packageCountOf(managerId)
}

function cacheBytes(managerId: string) {
  return store.cacheOf(managerId)
}
</script>

<template>
  <section class="scroll-area">
    <div class="col">
      <!-- 概览条 -->
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

      <!-- 按阶段分组 -->
      <template v-for="group in tiers" :key="group.tier">
        <div class="tier-title">
          <span>{{ tierTitle(group.tier) }}</span>
          <span class="hint">
            {{ group.items.filter((i) => i.detected).length }} / {{ group.items.length }}
          </span>
        </div>

        <div class="manager-grid">
          <article
            v-for="manager in group.items"
            :key="manager.id"
            class="manager-card"
            :class="{
              'is-missing': !manager.detected,
              'is-future': manager.tier > 1,
            }"
          >
            <header class="manager-card__head">
              <span class="dot" :class="manager.detected ? 'dot--ok' : 'dot--missing'" />
              <span class="manager-card__name">{{ manager.name }}</span>
              <span class="tag">{{ manager.language }}</span>
              <span class="panel__spacer" />
              <span v-if="manager.tier > 1" class="tag tag--warn">{{ t('nav.tierFuture') }}</span>
            </header>

            <div class="manager-card__body">
              <template v-if="manager.detected">
                <div class="kv">
                  <span class="kv__k">{{ t('manage.version') }}</span>
                  <span class="kv__v">{{ manager.version ?? '—' }}</span>
                  <span class="kv__k">{{ t('manage.globalRoot') }}</span>
                  <span class="kv__v" :title="manager.globalRoot ?? ''">
                    {{ ellipsisPath(manager.globalRoot, 44) }}
                  </span>
                  <span class="kv__k">{{ t('manage.cacheDir') }}</span>
                  <span class="kv__v" :title="manager.cacheDir ?? ''">
                    {{ ellipsisPath(manager.cacheDir, 44) }}
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

                <div v-if="manager.tier > 1" class="hint danger-text">{{ t('manage.futureNote') }}</div>
              </template>

              <template v-else>
                <div class="manager-card__missing">
                  <span>{{ t('manage.missing') }}</span>
                  <span class="hint">{{ t('manage.notDetectedHint') }}</span>
                </div>
                <div v-if="manager.warnings.length" class="hint">{{ manager.warnings[0] }}</div>
              </template>

              <!-- 展开的详细信息 -->
              <div v-if="expanded.has(manager.id)" class="manager-card__detail">
                <div class="kv">
                  <span class="kv__k">{{ t('manage.exePath') }}</span>
                  <span class="kv__v">{{ manager.exePath ?? '—' }}</span>
                  <span class="kv__k">{{ t('manage.configFile') }}</span>
                  <span class="kv__v">{{ manager.configFile ?? '—' }}</span>
                  <template v-if="!manager.detected && manager.downloadUrl">
                    <span class="kv__k">{{ t('manage.download') }}</span>
                    <span class="kv__v">{{ manager.downloadUrl }}</span>
                  </template>
                </div>
                <div v-for="warning in manager.warnings" :key="warning" class="hint danger-text">
                  {{ warning }}
                </div>
              </div>
            </div>

            <footer class="manager-card__foot">
              <button class="btn btn--ghost btn--sm" @click="toggleDetail(manager.id)">
                {{ expanded.has(manager.id) ? '−' : '+' }}
              </button>

              <template v-if="manager.detected">
                <button
                  class="btn btn--primary btn--sm"
                  :disabled="store.scanning || manager.tier > 1"
                  @click="manage(manager)"
                >
                  {{ t('manage.manage') }}
                </button>
              </template>

              <template v-else>
                <button
                  class="btn btn--primary btn--sm"
                  :disabled="!manager.downloadUrl"
                  @click="store.openManagedLink('manager', manager.id)"
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
      </template>

      <p class="hint">
        「前往官网下载」只会用系统浏览器打开官方页面，不会自动安装任何东西。
      </p>
    </div>
  </section>
</template>
