<script setup lang="ts">
/**
 * 设置页：语言 / 主题 / 启动行为 + 运行环境与安全模型说明。
 *
 * 语言与主题都会即时预览（改一下就能看到效果），点「保存设置」才写入磁盘。
 */
import { computed, onMounted, ref } from 'vue'
import { useSettingsStore, type ThemeName } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useI18n, type Locale } from '@/i18n'
import { api, IpcError } from '@/api'

const settings = useSettingsStore()
const store = useAppStore()
const { t } = useI18n()

const diagnostics = ref<Record<string, unknown> | null>(null)
const diagError = ref('')

const LANGUAGES: { value: Locale; label: string }[] = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en-US', label: 'English' },
]

const THEMES = computed<{ value: ThemeName; label: string }[]>(() => [
  { value: 'dark', label: t('settings.themeDark') },
  { value: 'light', label: t('settings.themeLight') },
  { value: 'system', label: t('settings.themeSystem') },
])

const dirty = ref(false)

onMounted(async () => {
  try {
    diagnostics.value = await api.diagnostics()
  } catch (e) {
    diagError.value = e instanceof IpcError ? e.message : String(e)
  }
})

function setLanguage(language: string) {
  settings.setLanguage(language)
  dirty.value = true
}

function setTheme(theme: ThemeName) {
  settings.setTheme(theme)
  dirty.value = true
}

function setToggle(key: 'scanOnStartup' | 'showIcons', value: boolean) {
  settings.patch({ [key]: value })
  dirty.value = true
}

async function save() {
  const result = await settings.persist()
  if (result.ok) {
    dirty.value = false
    store.notify('success', t('settings.saved'))
    store.pushLog(`设置已保存：${result.message}`)
  } else {
    store.notify('error', result.message ?? '')
  }
}
</script>

<template>
  <section class="scroll-area">
    <div class="col">
      <!-- 外观 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ t('settings.title') }}</span>
          <span class="panel__spacer" />
          <span v-if="dirty" class="tag tag--warn">未保存</span>
        </div>

        <div class="panel__body col">
          <!-- 语言 -->
          <div class="setting-row">
            <div class="setting-row__label">
              <div>{{ t('settings.language') }}</div>
              <div class="hint">{{ t('settings.languageHint') }}</div>
            </div>
            <div class="segmented">
              <button
                v-for="lang in LANGUAGES"
                :key="lang.value"
                class="segmented__item"
                :class="{ 'is-active': settings.settings.language === lang.value }"
                @click="setLanguage(lang.value)"
              >
                {{ lang.label }}
              </button>
            </div>
          </div>

          <!-- 主题 -->
          <div class="setting-row">
            <div class="setting-row__label">
              <div>{{ t('settings.theme') }}</div>
              <div class="hint">{{ t('settings.themeHint') }}</div>
            </div>
            <div class="segmented">
              <button
                v-for="theme in THEMES"
                :key="theme.value"
                class="segmented__item"
                :class="{ 'is-active': settings.settings.theme === theme.value }"
                @click="setTheme(theme.value)"
              >
                {{ theme.label }}
              </button>
            </div>
          </div>

          <!-- 开关 -->
          <div class="setting-row">
            <div class="setting-row__label">
              <div>{{ t('settings.scanOnStartup') }}</div>
              <div class="hint">{{ t('settings.scanOnStartupHint') }}</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="settings.settings.scanOnStartup"
                @change="setToggle('scanOnStartup', ($event.target as HTMLInputElement).checked)"
              />
              <span class="switch__track"><span class="switch__thumb" /></span>
            </label>
          </div>

          <div class="setting-row">
            <div class="setting-row__label">
              <div>{{ t('settings.showIcons') }}</div>
              <div class="hint">{{ t('settings.showIconsHint') }}</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="settings.settings.showIcons"
                @change="setToggle('showIcons', ($event.target as HTMLInputElement).checked)"
              />
              <span class="switch__track"><span class="switch__thumb" /></span>
            </label>
          </div>

          <div class="row" style="justify-content: flex-end">
            <span v-if="settings.error" class="hint danger-text" style="margin-right: auto">
              {{ settings.error }}
            </span>
            <button class="btn btn--primary" :disabled="settings.saving || !dirty" @click="save">
              {{ settings.saving ? t('settings.saving') : t('settings.save') }}
            </button>
          </div>
        </div>
      </div>

      <!-- 运行环境 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ t('settings.diagnostics') }}</span>
        </div>
        <div class="panel__body">
          <div v-if="diagError" class="banner banner--error">{{ diagError }}</div>
          <div v-else-if="!diagnostics" class="hint">…</div>
          <div v-else class="kv">
            <span class="kv__k">{{ t('settings.os') }}</span>
            <span class="kv__v">{{ diagnostics.os }} / {{ diagnostics.arch }}</span>
            <span class="kv__k">{{ t('settings.home') }}</span>
            <span class="kv__v">{{ diagnostics.home ?? '—' }}</span>
            <span class="kv__k">{{ t('settings.managers') }}</span>
            <span class="kv__v">
              {{ (diagnostics.supportedManagers as string[])?.join('、') }}
            </span>
          </div>
        </div>
      </div>

      <!-- 安全模型 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">{{ t('settings.safetyTitle') }}</span>
        </div>
        <div class="panel__body">
          <p class="hint" style="margin: 0">{{ t('settings.safetyBody') }}</p>
        </div>
      </div>
    </div>
  </section>
</template>
