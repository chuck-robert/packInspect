/**
 * 后端数据结构镜像。
 *
 * ⚠️ 与 `src-tauri/src/models.rs` 一一对应（Rust 侧 `rename_all = "camelCase"`）。
 * 任何字段调整必须两侧同步，否则 `npm run typecheck` 与 `cargo check` 会各报一半。
 */

/** 包管理器探测结果 */
export interface ManagerInfo {
  id: string
  name: string
  language: string
  detected: boolean
  version: string | null
  exePath: string | null
  globalRoot: string | null
  cacheDir: string | null
  configFile: string | null
  registry: RegistryConfig | null
  warnings: string[]
}

export interface RegistryEntry {
  key: string
  value: string
  userDefined: boolean
  hint: string | null
}

export interface RegistryConfig {
  managerId: string
  entries: RegistryEntry[]
  raw: string
  writable: boolean
}

export type PackageScope = 'global' | 'local' | 'system'

export interface PackageRecord {
  name: string
  version: string | null
  manager: string
  scope: PackageScope
  path: string | null
  size: number | null
  redundant: boolean
  redundantReason: string | null
  description: string | null
  latestVersion: string | null
}

export interface CacheChild {
  name: string
  path: string
  bytes: number
  fileCount: number
}

export interface CacheStats {
  managerId: string
  path: string
  exists: boolean
  totalBytes: number
  fileCount: number
  lastModified: string | null
  children: CacheChild[]
  truncated: boolean
}

export type CleanKind = 'cache' | 'oldversion' | 'temp' | 'orphan'
export type CleanRisk = 'safe' | 'warn' | 'protected'

export interface CleanCandidate {
  id: string
  managerId: string
  kind: CleanKind
  path: string
  bytes: number
  fileCount: number
  reason: string
  risk: CleanRisk
  protected: boolean
}

export interface CleanResult {
  candidateId: string
  path: string
  ok: boolean
  freedBytes: number
  message: string | null
}

export interface ScanReport {
  generatedAt: string
  hostname: string | null
  os: string
  managers: ManagerInfo[]
  packages: PackageRecord[]
  caches: CacheStats[]
  totalPackages: number
  totalCacheBytes: number
  durationMs: number
}

export interface ScanRequest {
  managers?: string[]
  measurePackageSize?: boolean
  timeoutMs?: number
}

export interface CleanRequest {
  candidateIds: string[]
  /** 默认 true：只预览不删除 */
  dryRun: boolean
}

export type ExportFormat = 'json' | 'csv' | 'markdown'

export interface ExportRequest {
  format: ExportFormat
  targetPath: string
}

/** 后端统一错误结构 */
export interface AppError {
  code: string
  message: string
}

/** 清理操作的两阶段状态机，供 UI 强制「预览 → 确认 → 执行」 */
export type CleanPhase = 'idle' | 'previewing' | 'previewed' | 'executing' | 'done'
