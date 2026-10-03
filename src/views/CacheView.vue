<script setup lang="ts">
/** 缓存占用视图：总体统计 + 一级子目录占用分布 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'
import { formatBytes, formatBytesShort, formatCount, formatDateTime, ellipsisPath } from '@/utils/format'

const store = useAppStore()
const { t } = useI18n()

const caches = computed(() => store.visibleCaches)
const maxBytes = computed(() => Math.max(1, ...caches.value.map((c) => c.totalBytes)))
const totalBytes = computed(() => caches.value.reduce((sum, c) => sum + c.totalBytes, 0))
const totalFiles = computed(() => caches.value.reduce((sum, c) => sum + c.fileCount, 0))
const largest = computed(() =>
  caches.value.length ? caches.value.reduce((a, b) => (a.totalBytes > b.totalBytes ? a : b)) : null,
)

function ratio(value: number, max: number) {
  return `${Math.min(100, Math.round((value / max) * 100))}%`
}
</script>

<template>
  <section class="scroll-area" style="padding: 12px 14px">
    <div v-if="caches.length === 0" class="empty">
      <div class="empty__icon">🗂️</div>
      <div>{{ t('cache.emptyTitle') }}</div>
      <div class="hint">{{ t('cache.emptyHint') }}</div>
    </div>

    <div v-else class="col">
      <!-- 汇总 -->
      <div class="stat-grid">
        <div class="stat">
          <div class="stat__label">{{ t('cache.total') }}</div>
          <div class="stat__value">{{ formatBytes(totalBytes) }}</div>
          <div class="stat__hint">{{ t('cache.files', { count: formatCount(totalFiles) }) }}</div>
        </div>
        <div class="stat">
          <div class="stat__label">{{ t('cache.managers') }}</div>
          <div class="stat__value">{{ caches.length }}</div>
          <div class="stat__hint">{{ caches.map((c) => c.managerId).join('、') }}</div>
        </div>
        <div class="stat">
          <div class="stat__label">{{ t('cache.largest') }}</div>
          <div class="stat__value">{{ formatBytesShort(largest?.totalBytes ?? 0) }}</div>
          <div class="stat__hint">{{ largest?.managerId }}</div>
        </div>
      </div>

      <!-- 每个管理器的缓存详情 -->
      <div v-for="c in caches" :key="c.managerId" class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ c.managerId }}</span>
          <span class="tag" :class="c.exists ? 'tag--ok' : 'tag--danger'">
            {{ c.exists ? t('cache.exists') : t('cache.missing') }}
          </span>
          <span class="panel__sub" :title="c.path">{{ ellipsisPath(c.path, 60) }}</span>
          <span class="panel__spacer" />
          <span v-if="c.truncated" class="tag tag--warn">{{ t('cache.truncated') }}</span>
          <span class="panel__sub">
            {{ t('cache.lastModified') }} {{ formatDateTime(c.lastModified) }}
          </span>
          <button class="btn btn--ghost btn--sm" @click="store.refreshCacheStats(c.managerId)">
            {{ t('cache.refresh') }}
          </button>
        </div>

        <div class="panel__body">
          <div class="row" style="margin-bottom: 10px">
            <div style="flex: 1">
              <div class="bar">
                <div class="bar__fill" :style="{ width: ratio(c.totalBytes, maxBytes) }" />
              </div>
            </div>
            <span class="mono" style="width: 90px; text-align: right">
              {{ formatBytes(c.totalBytes) }}
            </span>
            <span class="hint" style="width: 120px; text-align: right">
              {{ t('cache.files', { count: formatCount(c.fileCount) }) }}
            </span>
          </div>

          <div v-if="c.children.length === 0" class="hint">{{ t('cache.childEmpty') }}</div>

          <div v-else class="tree">
            <div v-for="child in c.children" :key="child.path" class="tree__row">
              <span style="width: 12px; color: var(--text-muted)">└</span>
              <span class="tree__name" :title="child.path">{{ child.name }}</span>
              <div class="bar" style="width: 120px">
                <div
                  class="bar__fill"
                  :style="{ width: ratio(child.bytes, Math.max(1, c.children[0].bytes)) }"
                />
              </div>
              <span class="tree__size" style="width: 78px; text-align: right">
                {{ formatBytesShort(child.bytes) }}
              </span>
              <span class="hint" style="width: 96px; text-align: right">
                {{ formatCount(child.fileCount) }}
              </span>
            </div>
          </div>
        </div>
      </div>

      <p class="hint">{{ t('cache.note') }}</p>
    </div>
  </section>
</template>
