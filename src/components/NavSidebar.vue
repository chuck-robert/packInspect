<script setup lang="ts">
/** 左侧导航：按语言生态分组的包管理器列表，带探测状态点 */
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { LANGUAGE_LABELS } from '@/utils/format'

const store = useAppStore()

const groups = computed(() => store.groupedManagers)

/** 语言分组的排序：有安装项的生态靠前 */
const orderedGroups = computed(() =>
  [...groups.value].sort((a, b) => {
    const aHit = a.items.some((i) => i.detected) ? 0 : 1
    const bHit = b.items.some((i) => i.detected) ? 0 : 1
    return aHit - bHit || a.language.localeCompare(b.language)
  }),
)

function statusClass(detected: boolean, warnings: string[]) {
  if (!detected) return 'dot--missing'
  return warnings.length ? 'dot--warn' : 'dot--ok'
}
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand__mark">PI</div>
      <div class="brand__text">
        <span class="brand__title">PackInspect</span>
        <span class="brand__sub">本地包环境扫描</span>
      </div>
    </div>

    <nav class="nav">
      <button
        class="nav__item"
        :class="{ 'is-active': store.activeManager === null }"
        @click="store.setActiveManager(null)"
      >
        <span class="dot" :class="store.installed.length ? 'dot--ok' : 'dot--missing'" />
        <span class="nav__label">全部</span>
        <span class="nav__version">{{ store.installed.length }}</span>
      </button>

      <template v-for="group in orderedGroups" :key="group.language">
        <div class="nav__group-title">
          {{ LANGUAGE_LABELS[group.language] ?? group.language }}
        </div>

        <button
          v-for="m in group.items"
          :key="m.id"
          class="nav__item"
          :class="{
            'is-active': store.activeManager === m.id,
            'is-missing': !m.detected,
          }"
          :title="
            m.detected
              ? `${m.exePath ?? ''}\n全局目录：${m.globalRoot ?? '未识别'}\n缓存目录：${m.cacheDir ?? '未识别'}`
              : '未检测到可执行文件'
          "
          @click="store.setActiveManager(m.id)"
        >
          <span class="dot" :class="statusClass(m.detected, m.warnings)" />
          <span class="nav__label">{{ m.name }}</span>
          <span class="nav__version">{{ m.version ?? (m.detected ? '?' : '—') }}</span>
        </button>
      </template>

      <div v-if="store.booting" class="hint" style="padding: 14px 8px">正在读取管理器定义…</div>
      <div v-else-if="store.detecting" class="hint" style="padding: 14px 8px">正在探测本机环境…</div>
    </nav>
  </aside>
</template>
