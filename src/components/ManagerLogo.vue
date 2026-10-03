<script setup lang="ts">
/**
 * 包管理器品牌 logo。
 *
 * 数据来自后端 `manager_logos`（内联 SVG data URI，按主题配色，后端有缓存）。
 * 组件本身不联网，也不拼 URL —— 拿不到时退回一个首字母方块，保证不出现空洞。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'

const props = withDefaults(
  defineProps<{
    managerId: string
    name: string
    /** sm = 侧边栏/搜索结果；md = 卡片；lg = 详情页头部 */
    size?: 'sm' | 'md' | 'lg'
  }>(),
  { size: 'md' },
)

const store = useAppStore()
const logo = ref<string | null>(null)
const failed = ref(false)

/** 先在 store 里找（探测阶段已经带回 logo），没有才单独取 */
const cached = computed(
  () => store.managers.find((m) => m.id === props.managerId)?.logo ?? null,
)

const letter = computed(() => {
  const source = (props.name || props.managerId || '?').trim()
  const cleaned = source.replace(/^@/, '')
  return cleaned.charAt(0).toUpperCase() || '?'
})

async function load() {
  if (cached.value) {
    logo.value = cached.value
    failed.value = false
    return
  }
  try {
    // 主题从当前生效值取，避免 store 循环依赖
    const theme = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark'
    const list = await store.loadLogos(theme, [props.managerId])
    const hit = list.find((item) => item.managerId === props.managerId)
    logo.value = hit?.dataUri ?? null
    failed.value = !logo.value
  } catch {
    failed.value = true
  }
}

watch(() => props.managerId, () => void load())
watch(cached, (value) => {
  if (value) logo.value = value
})

onMounted(() => void load())
</script>

<template>
  <img
    v-if="logo && !failed"
    class="mgr-logo"
    :class="`mgr-logo--${size}`"
    :src="logo"
    :alt="name"
    draggable="false"
  />
  <span v-else class="mgr-logo mgr-logo--fallback" :class="`mgr-logo--${size}`">
    {{ letter }}
  </span>
</template>
