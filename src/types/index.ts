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
  /** 1 = 一期可用，2/3/4 = 后续阶段（界面弱化展示） */
  tier: number
  /** 适用平台说明，例如「仅 Linux」「macOS / Linux」 */
  platforms: string
  /**
   * 是否适用于**当前**操作系统。
   * false 表示这东西在本机不可能存在（如 Windows 上的 apt / brew），
   * 界面要说明原因，而不是提示「未在 PATH 中找到」。
   */
  platformApplicable: boolean
  detected: boolean
  version: string | null
  exePath: string | null
  globalRoot: string | null
  cacheDir: string | null
  configFile: string | null
  registry: RegistryConfig | null
  /** 包管理器品牌 logo（内联 SVG data URI，按当前主题配色） */
  logo: string | null
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
}

/** 右键菜单里的管理动作 */
export interface ManagementAction {
  action: 'manage' | 'update' | 'uninstall' | 'install' | 'disable' | 'openDocs' | 'inspect'
  label: string
  online: boolean
  destructive: boolean
  /**
   * 是否可执行。
   *
   * true 表示白名单里确实注册了该操作模板，点击后会**真的改动你的环境**
   * （走 `run_package_op`，带确认与超时）；
   * false 表示该生态没有可靠做法，界面会说明原因而不是假装可用。
   */
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

/**
 * 磁盘快照：后端持久化的"上次状态"，用于首屏秒开。
 *
 * 由前端在探测/扫描结束后保存它**正在显示**的那份数据，因此恢复出来
 * 一定与用户上次看到的一致。
 */
export interface Snapshot {
  /** 结构版本；后端版本不匹配时会直接丢弃快照 */
  schema: number
  /** 采集时间（RFC3339） */
  capturedAt: string
  managers: ManagerInfo[]
  /** 可能为 null：上次从未扫描过 */
  report: ScanReport | null
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
  /** 是否列出「当前系统不适用」的包管理器（如 Windows 上的 apt / brew），默认关闭 */
  showOtherPlatforms: boolean
}

/** 后端统一错误结构 */
export interface AppError {
  code: string
  message: string
}

/** 清理操作的两阶段状态机，供 UI 强制「预览 → 确认 → 执行」 */
export type CleanPhase = 'idle' | 'previewing' | 'previewed' | 'executing' | 'done'

/**
 * 顶层视图。
 *
 * 注意 `manager`：点击某个包管理器后进入该管理器的详情视图（含分页），
 * 而不是直接跳到包列表 —— 这样用户先看到「它是什么、装在哪、版本多少」，
 * 再从分页进入包列表 / 浏览安装 / 管理操作。
 */
export type ViewKey = 'manage' | 'manager' | 'cache' | 'registry' | 'settings'

/** 管理器详情页内的分页 */
export type ManagerTab = 'overview' | 'packages' | 'browse' | 'manage'

/** 包管理器品牌 logo（按主题批量获取） */
export interface ManagerLogo {
  managerId: string
  dataUri: string
}

export interface LogoRequest {
  /** dark | light */
  theme: string
  /** 为空表示全部 */
  managers?: string[]
}

/** 在线仓库里的一个可安装包 */
export interface RemotePackage {
  name: string
  version: string | null
  description: string | null
  /** 下载量 / 热度 */
  downloads: number | null
  /** 包主页地址 */
  homepage: string | null
  /** 等价安装命令（也用于执行前展示给用户核对） */
  installCommand: string | null
}

export interface BrowseRequest {
  manager: string
  query: string
  limit?: number
  timeoutMs?: number
}

/**
 * 安装方案：包含确切命令与作用域说明。
 *
 * 它本身不执行任何东西；要执行需再经确认对话框走 `run_package_op`。
 * `explanation` 说明这条命令做什么、有什么范围限制（例如 dotnet 只能作用于当前项目）。
 */
export interface InstallPlan {
  managerId: string
  package: string
  command: string
  online: boolean
  requiresAdmin: boolean
  explanation: string
}

/**
 * 在线浏览的结果。
 *
 * 不直接返回数组是因为空数组无法区分「真的没有」「网络失败」「不支持搜索」——
 * 界面需要对这三种情况给出完全不同的提示。
 */
export interface BrowseResult {
  packages: RemotePackage[]
  /** 是否真的发起了查询（false = 该生态不支持关键词搜索） */
  attempted: boolean
  /** 是否发生网络 / 解析失败 */
  failed: boolean
  /** 失败或限制说明 */
  note: string | null
  /** 使用提示，例如「PyPI 不支持关键词搜索，已按精确名查询」 */
  hint: string | null
}

/** 单个管理器的扫描结果（渐进式扫描的返回单元） */
export interface ManagerScanResult {
  managerId: string
  packages: PackageRecord[]
  cache: CacheStats | null
  durationMs: number
  /** 是否成功读取；false 时 packages 为空且 reason 有值 */
  ok: boolean
  reason: string | null
}

/** 一次真实包管理操作的结果 */
export interface PackageOpResult {
  managerId: string
  package: string
  /** update | uninstall | install */
  action: string
  /** 等价命令（展示用，不用于执行） */
  command: string
  success: boolean
  timedOut: boolean
  exitCode: number | null
  stdout: string
  stderr: string
  message: string | null
  durationMs: number
  /** 执行日志的落盘位置 */
  logPath: string | null
}

/** 全局搜索命中的一条结果 */
export interface SearchHit {
  kind: 'manager' | 'package'
  managerId: string
  managerName: string
  label: string
  detail: string
  /** 包命中时的原始记录，用于直接打开详情 */
  record?: PackageRecord
}

