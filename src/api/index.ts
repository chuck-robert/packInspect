/**
 * Tauri IPC 封装层。
 *
 * 这里是前端与系统的**唯一**通道：
 * - 前端永远不接触 shell / 文件系统
 * - 只发送结构化意图（manager id、配置键、绝对路径）
 * - 统一错误归一化，让 UI 只处理 `AppError`
 */

import { invoke } from '@tauri-apps/api/core'
import type {
  AppError,
  AppSettings,
  CacheStats,
  CleanCandidate,
  CleanRequest,
  CleanResult,
  ExportRequest,
  IconResponse,
  InstallHint,
  ManagementAction,
  ManagerInfo,
  OpenLinkRequest,
  PackageRecord,
  PluginNode,
  PluginsRequest,
  RegistryConfig,
  ScanReport,
  ScanRequest,
} from '@/types'

/** 后端错误归一化：网络层异常也包装成同样的形状 */
export class IpcError extends Error {
  code: string

  constructor(code: string, message: string) {
    super(message)
    this.name = 'IpcError'
    this.code = code
  }

  static from(raw: unknown): IpcError {
    if (raw && typeof raw === 'object' && 'code' in raw && 'message' in raw) {
      const e = raw as AppError
      return new IpcError(e.code, e.message)
    }
    if (raw instanceof Error) {
      return new IpcError('IPC_ERROR', raw.message)
    }
    return new IpcError('IPC_ERROR', String(raw))
  }

  /** 是否属于「没安装」这类可预期状态，UI 应展示空态而非报错 */
  get isExpected(): boolean {
    return ['NOT_INSTALLED', 'EMPTY_OUTPUT', 'STALE_SELECTION'].includes(this.code)
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (raw) {
    throw IpcError.from(raw)
  }
}

export interface SupportedManager {
  id: string
  name: string
  language: string
  allowedOps: string[]
}

export const api = {
  /** 静态列表：不触发任何系统调用，用于首屏骨架 */
  supportedManagers: () => call<SupportedManager[]>('supported_managers'),

  /** 探测本机包管理器；force = 忽略 5 分钟缓存 */
  detectManagers: (force = false, timeoutMs?: number) =>
    call<ManagerInfo[]>('detect_managers', { force, timeoutMs }),

  getRegistry: (managerId: string, timeoutMs?: number) =>
    call<RegistryConfig>('get_registry', { managerId, timeoutMs }),

  getAllRegistries: (timeoutMs?: number) =>
    call<Record<string, RegistryConfig>>('get_all_registries', { timeoutMs }),

  /** 写入镜像源；后端会先备份原配置文件 */
  setRegistry: (managerId: string, key: string, value: string) =>
    call<string>('set_registry', { managerId, key, value }),

  /** 预览将要写入的配置内容（不落盘） */
  previewRegistryChange: (managerId: string, key: string, value: string, original: string) =>
    call<string>('preview_registry_change', { managerId, key, value, original }),

  runScan: (request: ScanRequest) => call<ScanReport>('run_scan', { request }),

  getCacheStats: (managerId: string) => call<CacheStats>('get_cache_stats', { managerId }),

  /** 只读枚举清理候选，绝不删除 */
  listCleanCandidates: (timeoutMs?: number) =>
    call<CleanCandidate[]>('list_clean_candidates', { timeoutMs }),

  /**
   * 清理。`dryRun: true` 只算账；真正删除必须显式传 `dryRun: false`，
   * 且调用方需已完成二次确认。
   */
  cleanCaches: (request: CleanRequest, timeoutMs?: number) =>
    call<CleanResult[]>('clean_caches', { request, timeoutMs }),

  exportReport: (reportData: ScanReport, request: ExportRequest) =>
    call<string>('export_report', { reportData, request }),

  diagnostics: () => call<Record<string, unknown>>('get_diagnostics'),

  parentDir: (path: string) => call<string>('parent_dir', { path }),

  // ---------------------------------------------------------------- 包管理交互

  /**
   * 取包图标。后端返回内联 SVG data URI（离线生成，按包名哈希配色），
   * 同一 (manager, package) 在后端有缓存，重复调用零成本。
   */
  packageIcon: (managerId: string, packageName: string) =>
    call<IconResponse>('package_icon', { managerId, package: packageName }),

  /**
   * 取右键菜单的管理动作。
   * 一期只有 manage / inspect / openDocs 可用；update / uninstall / install
   * 会返回等价命令但 `enabled = false`（占位，不会真的执行）。
   */
  packageActions: (managerId: string, packageName: string, scope?: string) =>
    call<ManagementAction[]>('package_actions', { managerId, package: packageName, scope }),

  /** 展开包内子节点（插件 / 依赖 / 文件） */
  packagePlugins: (request: PluginsRequest) => call<PluginNode[]>('package_plugins', { request }),

  /** 未检测到的包管理器 + 官方下载入口 */
  installHints: () => call<InstallHint[]>('install_hints'),

  /**
   * 用系统浏览器打开链接。
   * kind = 'manager' 时由后端从白名单取官网地址，前端无法传任意 URL；
   * kind = 'url' 时后端仍会校验 https + 域名白名单。
   */
  openExternalLink: (request: OpenLinkRequest) =>
    call<string>('open_external_link', { request }),

  // ---------------------------------------------------------------- 设置

  getSettings: () => call<AppSettings>('get_settings'),

  saveSettings: (settings: AppSettings) => call<string>('save_settings', { settingsData: settings }),
}

export type { PackageRecord }
