<script setup lang="ts">
/** 缓存占用视图：总体统计 + 一级子目录占用分布 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { formatBytes, formatBytesShort, formatCount, formatDateTime, ellipsisPath } from '@/utils/format'

const store = useAppStore()

const caches = computed(() => store.visibleCaches)
const maxBytes = computed(() => Math.max(1, ...caches.value.map((c) => c.totalBytes)))
const totalBytes = computed(() => caches.value.reduce((s, c) => s + c.totalBytes, 0))
const totalFiles = computed(() => caches.value.reduce((s, c) => s + c.fileCount, 0))

function ratio(value: number, max: number) {
  return `${Math.min(100, Math.round((value / max) * 100))}%`
}
</script>

<template>
  <section class="scroll-area" style="padding: 12px 14px">
    <div v-if="caches.length === 0" class="empty">
      <div class="empty__icon">🗂️</div>
      <div>还没有缓存数据</div>
      <div class="hint">执行一次扫描即可统计缓存目录占用。缓存统计只读取文件大小，不会修改任何文件。</div>
    </div>

    <div v-else class="col">
      <!-- 汇总 -->
      <div class="stat-grid">
        <div class="stat">
          <div class="stat__label">缓存总占用</div>
          <div class="stat__value">{{ formatBytes(totalBytes) }}</div>
          <div class="stat__hint">{{ formatCount(totalFiles) }} 个文件</div>
        </div>
        <div class="stat">
          <div class="stat__label">涉及管理器</div>
          <div class="stat__value">{{ caches.length }}</div>
          <div class="stat__hint">{{ caches.map((c) => c.managerId).join('、') }}</div>
        </div>
        <div class="stat">
          <div class="stat__label">占用最大</div>
          <div class="stat__value">
            {{ formatBytesShort(Math.max(...caches.map((c) => c.totalBytes))) }}
          </div>
          <div class="stat__hint">
            {{ caches.reduce((a, b) => (a.totalBytes > b.totalBytes ? a : b)).managerId }}
          </div>
        </div>
      </div>

      <!-- 每个管理器的缓存详情 -->
      <div v-for="c in caches" :key="c.managerId" class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ c.managerId }}</span>
          <span class="tag" :class="c.exists ? 'tag--ok' : 'tag--danger'">
            {{ c.exists ? '存在' : '目录不存在' }}
          </span>
          <span class="panel__sub" :title="c.path">{{ ellipsisPath(c.path, 60) }}</span>
          <span class="panel__spacer" />
          <span v-if="c.truncated" class="tag tag--warn">已截断统计</span>
          <span class="panel__sub">最近修改 {{ formatDateTime(c.lastModified) }}</span>
          <button class="btn btn--ghost btn--sm" @click="store.refreshCacheStats(c.managerId)">
            刷新
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
            <span class="hint" style="width: 110px; text-align: right">
              {{ formatCount(c.fileCount) }} 文件
            </span>
          </div>

          <div v-if="c.children.length === 0" class="hint">目录为空或无法读取。</div>

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
              <span class="hint" style="width: 84px; text-align: right">
                {{ formatCount(child.fileCount) }} 文件
              </span>
            </div>
          </div>
        </div>
      </div>

      <p class="hint">
        提示：以上仅为「占用统计」。删除缓存需要进入工具栏的「清理缓存」，那里会先预览将删除的内容并要求二次确认。
      </p>
    </div>
  </section>
</template>
