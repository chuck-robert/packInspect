<script setup lang="ts">
/** 全局提示区：错误条 / 成功提示 / 当前管理器的探测警告 */
import { computed, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from '@/i18n'

const store = useAppStore()
const { t } = useI18n()

const activeWarnings = computed(() => {
  const manager = store.managers.find((m) => m.id === store.activeManager)
  return manager?.warnings ?? []
})

const visible = ref(false)

// toast 自动消失（错误条不自动消失，必须用户确认）
watch(
  () => store.toast,
  (toast) => {
    visible.value = !!toast
    if (!toast) return
    if (toast.kind === 'error') return
    window.setTimeout(() => {
      if (store.toast === toast) {
        store.clearToast()
        visible.value = false
      }
    }, 4200)
  },
  { immediate: true },
)

const warningText = computed(() => `${t('banner.detectWarn')}: ${activeWarnings.value.join('; ')}`)
</script>

<template>
  <div
    v-if="store.error || (store.toast && visible) || activeWarnings.length"
    class="col"
    style="padding: 10px 14px 0; gap: 6px"
  >
    <div v-if="store.error" class="banner banner--error">
      <strong>{{ store.error.code }}</strong>
      <span>{{ store.error.message }}</span>
      <span class="banner__spacer" />
      <button class="btn btn--ghost btn--sm" @click="store.clearError()">{{ t('banner.close') }}</button>
    </div>

    <div v-if="store.toast && visible" class="banner" :class="`banner--${store.toast.kind}`">
      <span>{{ store.toast.message }}</span>
      <span class="banner__spacer" />
      <button
        class="btn btn--ghost btn--sm"
        @click="
          () => {
            store.clearToast()
            visible = false
          }
        "
      >
        {{ t('banner.close') }}
      </button>
    </div>

    <div v-if="activeWarnings.length" class="banner banner--warn">
      <span>{{ warningText }}</span>
    </div>
  </div>
</template>
