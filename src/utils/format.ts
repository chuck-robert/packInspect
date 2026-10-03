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
