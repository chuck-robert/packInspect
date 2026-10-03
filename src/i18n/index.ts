/**
 * 国际化。
 *
 * 文案放在 `locales/*.json`，与代码分离：
 * - 翻译者可以直接改 JSON，不需要碰 TypeScript
 * - 两种语言的键在**模块加载时**校验，缺翻译会立刻抛错而不是显示一堆键名
 * - 新增语言只需加一个 JSON 文件并登记到 `DICTS`
 *
 * 不引入 vue-i18n 的理由：本项目只需要「键值查找 + {name} 插值」，
 * 一个 60 行的实现比 30KB 依赖更划算，也更容易看出哪些文案缺翻译。
 *
 * 用法：
 *   const { t } = useI18n()
 *   t('nav.packages')
 *   t('manage.packageCount', { count: 12 })
 */

import { computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import zhCN from './locales/zh-CN.json'
import enUS from './locales/en-US.json'

export type Locale = 'zh-CN' | 'en-US'

type Dict = Record<string, string>

const DICTS: Record<Locale, Dict> = {
  'zh-CN': zhCN as Dict,
  'en-US': enUS as Dict,
}

/**
 * 键一致性校验：以中文为基准，另一种语言缺键或多键都直接抛错。
 *
 * 刻意放在模块顶层而不是测试里 —— 缺翻译是「打开界面就能看到」的问题，
 * 让它在这里立刻失败，比等到用户看见键名更容易发现。
 */
function assertSameKeys(base: Locale, other: Locale) {
  const baseKeys = new Set(Object.keys(DICTS[base]))
  const otherKeys = new Set(Object.keys(DICTS[other]))
  const missing = [...baseKeys].filter((key) => !otherKeys.has(key))
  const extra = [...otherKeys].filter((key) => !baseKeys.has(key))
  if (missing.length || extra.length) {
    const detail = [
      missing.length ? `缺少 ${missing.length} 条: ${missing.slice(0, 8).join(', ')}` : '',
      extra.length ? `多余 ${extra.length} 条: ${extra.slice(0, 8).join(', ')}` : '',
    ]
      .filter(Boolean)
      .join(' / ')
    throw new Error(`[i18n] ${other} 与 ${base} 的文案键不一致 —— ${detail}`)
  }
}

assertSameKeys('zh-CN', 'en-US')

/** 取当前语言下的文案；缺失时回退到中文，再回退到 key 本身 */
export function translate(
  locale: Locale,
  key: string,
  params?: Record<string, string | number>,
): string {
  const raw = DICTS[locale]?.[key] ?? DICTS['zh-CN'][key] ?? key
  if (!params) return raw
  return raw.replace(/\{(\w+)\}/g, (_, name: string) =>
    params[name] === undefined ? `{${name}}` : String(params[name]),
  )
}

/** 当前可用的语言列表（设置页渲染用） */
export const AVAILABLE_LOCALES: { value: Locale; label: string }[] = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en-US', label: 'English' },
]

/** 组合式：在组件里 `const { t, locale } = useI18n()` */
export function useI18n() {
  const settings = useSettingsStore()
  const locale = computed<Locale>(() =>
    settings.settings.language === 'en-US' ? 'en-US' : 'zh-CN',
  )
  const t = (key: string, params?: Record<string, string | number>) =>
    translate(locale.value, key, params)
  return { t, locale }
}

/** 供非组件环境（例如 store）使用 */
export function currentLocale(): Locale {
  try {
    const settings = useSettingsStore()
    return settings.settings.language === 'en-US' ? 'en-US' : 'zh-CN'
  } catch {
    return 'zh-CN'
  }
}
