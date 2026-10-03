import type { CleanKind, CleanRisk, PackageScope } from '@/types'

/** 字节 → 人类可读，与 Rust 侧 `fsutil::human_bytes` 保持一致 */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return '—'
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let idx = 0
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024
    idx += 1
  }
  return `${value.toFixed(2)} ${units[idx]}`
}

/** 紧凑体积（表格用），例：1.2G */
export function formatBytesShort(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return '—'
  if (bytes < 1024) return `${bytes}B`
  const units = ['K', 'M', 'G', 'T']
  let value = bytes / 1024
  let idx = 0
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024
    idx += 1
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)}${units[idx]}`
}

export function formatDateTime(input: string | null | undefined): string {
  if (!input) return '—'
  const date = new Date(input)
  if (Number.isNaN(date.getTime())) return input
  return date.toLocaleString('zh-CN', { hour12: false })
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`
  return `${(ms / 1000).toFixed(1)} s`
}

/** 数字千分位 */
export function formatCount(n: number): string {
  return n.toLocaleString('zh-CN')
}

export const LANGUAGE_LABELS: Record<string, string> = {
  node: 'Node.js',
  python: 'Python',
  rust: 'Rust',
  go: 'Go',
  ruby: 'Ruby',
}

export const SCOPE_LABELS: Record<PackageScope, string> = {
  global: '全局',
  local: '项目局部',
  system: '基础环境',
}

export const CLEAN_KIND_LABELS: Record<CleanKind, string> = {
  cache: '缓存目录',
  oldversion: '旧版本包',
  temp: '临时残留',
  orphan: '孤立文件',
}

export const CLEAN_RISK_LABELS: Record<CleanRisk, string> = {
  safe: '安全',
  warn: '需注意',
  protected: '受保护',
}

/** 截断长路径，保留头尾便于识别 */
export function ellipsisPath(path: string | null, max = 64): string {
  if (!path) return '—'
  if (path.length <= max) return path
  const head = Math.ceil((max - 3) / 2)
  const tail = Math.floor((max - 3) / 2)
  return `${path.slice(0, head)}...${path.slice(-tail)}`
}

/** 简易防抖，用于搜索框 */
export function debounce<T extends (...args: never[]) => void>(fn: T, wait = 200) {
  let timer: ReturnType<typeof setTimeout> | undefined
  return (...args: Parameters<T>) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => fn(...args), wait)
  }
}

/**
 * 从管理器的 `--version` 输出里只取出版本号。
 *
 * 各管理器的输出格式并不统一，实测样本：
 *   `11.13.0`                                   → 11.13.0
 *   `cargo 1.99.0 (5f94df478 2026-08-27)`       → 1.99.0
 *   `pip 26.1.2 from C:\...\site-packages\pip`  → 26.1.2
 *   `Apache Maven 3.9.16 (2bdd9fdda 2025-...)`  → 3.9.16
 *   `v1.29.380`                                 → 1.29.380
 *   `5.1.26100.9444`                            → 5.1.26100.9444
 *
 * 侧边栏空间有限，只显示版本号本身，不显示构建哈希、日期或路径。
 */
export function shortVersion(rawVersion: string | null | undefined, managerId = ''): string {
  const raw = (rawVersion ?? '').trim()
  if (!raw) return ''

  // 1) 去掉开头的管理器名，如 "cargo 1.99.0" → "1.99.0"
  let rest = raw
  if (managerId) {
    const prefix = `${managerId} `
    if (rest.toLowerCase().startsWith(prefix.toLowerCase())) {
      rest = rest.slice(prefix.length).trim()
    }
  }
  // 2) 去掉其它常见前缀词
  rest = rest.replace(/^(Apache\s+)?Maven\s+/i, '')

  // 3) 第一个 token 就是版本号时直接用
  const token = rest.split(/\s+/)[0] ?? ''
  if (/^v?\d+(\.\d+)*([-+][\w.]+)?$/.test(token)) {
    return token.replace(/^v/, '')
  }
  // 4) 否则在整串里搜第一个版本号样式的片段
  const found = rest.match(/v?(\d+(?:\.\d+){1,3}(?:[-+][\w.]+)?)/)
  return found ? found[1] : ''
}

/**
 * 搜索用的归一化：小写 + 去掉空格与常见连接符。
 *
 * 为什么需要：同一个包在不同生态里有多种写法，用户不会记准确形式。
 * 例如 winget 的 Oh My Posh：显示名 `Oh My Posh`、包 ID `JanDeDobbeleer.OhMyPosh`，
 * 用户可能输入 `ohmyposh` / `oh my posh` / `Oh-My-Posh` —— 归一化后都能命中。
 */
export function normalizeForSearch(input: string): string {
  return input.toLowerCase().replace(/[\s\-_.]+/g, '')
}

/**
 * 判断一条记录是否命中关键字。
 *
 * 之前只匹配 `name` 与 `version`，导致「按显示名搜不到包」——
 * 不少生态（winget、cargo、dotnet）把用户可读的名称放在 description 里，
 * 而 name 是机器 ID（如 `JanDeDobbeleer.OhMyPosh`）。这里把 description 也纳入匹配。
 */
export function matchesKeyword(
  fields: (string | null | undefined)[],
  keyword: string,
): boolean {
  const needle = normalizeForSearch(keyword)
  if (!needle) return true
  return fields.some((field) => !!field && normalizeForSearch(field).includes(needle))
}

