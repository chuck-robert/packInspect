/**
 * 设置 store：语言 / 主题 / 启动行为。
 *
 * 单独拆出来是为了打破循环依赖：`app` store 需要读设置来决定是否自动扫描，
 * 而 i18n 的 `t()` 需要读语言 —— 如果都塞在 app store 里会互相引用。
 */

import { defineStore } from 'pinia'
import { useAppStore } from '@/stores/app'
import { api, IpcError } from '@/api'
import type { AppSettings } from '@/types'

export type ThemeName = 'dark' | 'light' | 'system'

interface State {
  settings: AppSettings
  loaded: boolean
  saving: boolean
  error: string | null
  /** 系统是否偏好深色（theme = system 时用） */
  systemDark: boolean
}

const DEFAULTS: AppSettings = {
  language: 'zh-CN',
  theme: 'dark',
  scanOnStartup: true,
  showOtherPlatforms: false,
}

export const useSettingsStore = defineStore('settings', {
  state: (): State => ({
    settings: { ...DEFAULTS },
    loaded: false,
    saving: false,
    error: null,
    systemDark: true,
  }),

  getters: {
    /** 实际生效的主题（把 system 解析成 dark/light） */
    resolvedTheme(state): 'dark' | 'light' {
      if (state.settings.theme === 'system') {
        return state.systemDark ? 'dark' : 'light'
      }
      return state.settings.theme === 'light' ? 'light' : 'dark'
    },
  },

  actions: {
    /** 监听系统主题变化，供 theme = system 时联动 */
    initSystemTheme() {
      if (typeof window === 'undefined' || !window.matchMedia) return
      const query = window.matchMedia('(prefers-color-scheme: dark)')
      this.systemDark = query.matches
      query.addEventListener('change', (event) => {
        this.systemDark = event.matches
        if (this.settings.theme === 'system') this.applyTheme()
      })
    },

    /** 把主题写到 <html data-theme>，CSS 变量据此切换 */
    applyTheme() {
      const theme = this.resolvedTheme
      document.documentElement.dataset.theme = theme
      // 让原生控件（滚动条、表单）也跟着走
      document.documentElement.style.colorScheme = theme
      // 缓存一份，供 main.ts 在挂载前预防闪色
      try {
        localStorage.setItem('packinspect.theme', theme)
      } catch {
        /* 隐私模式等场景下 localStorage 不可用，忽略 */
      }
    },

    /**
     * 把与「列表展示」相关的设置同步给 app store。
     *
     * 为什么不让 app store 直接读 settings store：两个 store 互相 import 会形成
     * 循环依赖。这里由设置侧单向推送，职责也更清楚。
     */
    syncToList() {
      try {
        useAppStore().showOtherPlatforms = this.settings.showOtherPlatforms
      } catch {
        /* 应用尚未初始化时忽略 */
      }
    },

    async load() {
      try {
        const loaded = await api.getSettings()
        this.settings = { ...DEFAULTS, ...loaded }
      } catch (e) {
        const err = e instanceof IpcError ? e : IpcError.from(e)
        this.error = err.message
        this.settings = { ...DEFAULTS }
      } finally {
        this.loaded = true
        this.applyTheme()
        this.syncToList()
      }
    },

    /** 即时预览（不落盘），用于设置页里切一下就看到效果 */
    patch(partial: Partial<AppSettings>) {
      this.settings = { ...this.settings, ...partial }
      if (partial.theme !== undefined) this.applyTheme()
      // 列表展示类设置要立刻生效，不用等保存
      if (partial.showOtherPlatforms !== undefined) this.syncToList()
    },

    async persist(): Promise<{ ok: boolean; message?: string }> {
      this.saving = true
      this.error = null
      try {
        const path = await api.saveSettings(this.settings)
        this.applyTheme()
        return { ok: true, message: path }
      } catch (e) {
        const err = e instanceof IpcError ? e : IpcError.from(e)
        this.error = err.message
        return { ok: false, message: err.message }
      } finally {
        this.saving = false
      }
    },

    setTheme(theme: ThemeName) {
      this.patch({ theme })
    },

    setLanguage(language: string) {
      this.patch({ language })
    },
  },
})
