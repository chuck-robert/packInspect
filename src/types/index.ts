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
  /** 1 = 一期可用，2/3 = 后续阶段（界面弱化展示） */
  tier: number
  detected: boolean
  version: string | null
  exePath: string | null
  globalRoot: string | null
  cacheDir: string | null
  configFile: string | null
  registry: RegistryConfig | null
  downloadUrl: string | null
  docsUrl: string | null
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

/** 包内子节点：插件 / 扩展 / 依赖 / 文件 */
export interface PluginNode {
  nodeType: 'plugin' | 'dependency' | 'file' | 'dir' | 'runtime'
  name: string
  version: string | null
  path: string | null
  size: number | null
  note: string | null
}

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
  /** 按需加载：未加载时为空数组 */
  plugins: PluginNode[]
  pluginsLoaded: boolean
  /** data URI 图标（后端生成，离线可用） */
  icon: string | null
}

/** 右键菜单里的管理动作 */
export interface ManagementAction {
  action: 'manage' | 'update' | 'uninstall' | 'install' | 'disable' | 'openDocs' | 'inspect'
  label: string
  online: boolean
  destructive: boolean
  /** false = 一期占位按钮，不会真的执行 */
  enabled: boolean
  /** 等价官方命令，仅供参考 */
  commandHint: string | null
  note: string | null
}

/** 未安装管理器的下载引导 */
export interface InstallHint {
  managerId: string
  name: string
  language: string
  downloadUrl: string | null
  docsUrl: string | null
  installHint: string | null
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

export interface PluginsRequest {
  manager: string
  package: string
  version?: string | null
  path?: string | null
  timeoutMs?: number
}

export interface OpenLinkRequest {
  /** manager = 打开该管理器官网；docs = 文档；url = 显式地址（后端仍会校验） */
  kind: 'manager' | 'docs' | 'url'
  target: string
}

export interface AppSettings {
  language: string
  theme: string
  scanOnStartup: boolean
  showIcons: boolean
}

export interface IconResponse {
  key: string
  dataUri: string
  cached: boolean
}

/** 后端统一错误结构 */
export interface AppError {
  code: string
  message: string
}

/** 清理操作的两阶段状态机，供 UI 强制「预览 → 确认 → 执行」 */
export type CleanPhase = 'idle' | 'previewing' | 'previewed' | 'executing' | 'done'

/** 导航视图 */
export type ViewKey = 'manage' | 'packages' | 'cache' | 'registry' | 'settings'
