/**
 * 全局状态。单一 store 足够：数据量不大，且各视图之间强耦合（扫描结果 → 管理页 / 表格 / 缓存 / 清理）。
 */

import { defineStore } from 'pinia'
import { api, IpcError, type SupportedManager } from '@/api'
import { matchesKeyword, shortVersion } from '@/utils/format'
import type {
  CacheStats,
  CleanCandidate,
  CleanPhase,
  CleanResult,
  ExportFormat,
  InstallHint,
  InstallPlan,
  ManagementAction,
  ManagerInfo,
  PackageRecord,
  ManagerTab,
  PluginNode,
  RemotePackage,
  ScanReport,
  SearchHit,
  ViewKey,
} from '@/types'

interface State {
  /** 静态定义（首屏骨架） */
  supported: SupportedManager[]
  /** 探测结果 */
  managers: ManagerInfo[]
  report: ScanReport | null
  caches: CacheStats[]
  candidates: CleanCandidate[]
  /** 未安装管理器的下载引导 */
  hints: InstallHint[]
  /** 清理流程的两阶段状态机 */
  cleanPhase: CleanPhase
  previewResults: CleanResult[]

  /** 按包名缓存的右键动作（避免每次右键都请求后端） */
  actionsCache: Record<string, ManagementAction[]>
  /** 按 `manager/name` 缓存的包内子节点 */
  pluginsCache: Record<string, PluginNode[]>
  pluginsLoading: string | null

  booting: boolean
  detecting: boolean
  scanning: boolean
  loadingCandidates: boolean
  savingRegistry: boolean

  /** 当前视图，默认进入「包管理」页 */
  view: ViewKey
  /** 管理器详情页内的分页，默认先看「概览」 */
  managerTab: ManagerTab
  /** 当前选中的管理器（null = 全部） */
  activeManager: string | null
  /** 在线浏览结果 */
  remotePackages: RemotePackage[]
  browseQuery: string
  browseLoading: boolean
  browseError: string | null
  /** 最近一次生成的安装方案 */
  installPlan: InstallPlan | null
  keyword: string
  onlyRedundant: boolean

  error: { code: string; message: string } | null
  toast: { kind: 'info' | 'success' | 'warn' | 'error'; message: string } | null
  log: string[]
}

export const useAppStore = defineStore('app', {
  state: (): State => ({
    supported: [],
    managers: [],
    report: null,
    caches: [],
    candidates: [],
    hints: [],
    cleanPhase: 'idle',
    previewResults: [],
    actionsCache: {},
    pluginsCache: {},
    pluginsLoading: null,
    booting: false,
    detecting: false,
    scanning: false,
    loadingCandidates: false,
    savingRegistry: false,
    view: 'manage',
    managerTab: 'overview',
    activeManager: null,
    remotePackages: [],
    browseQuery: '',
    browseLoading: false,
    browseError: null,
    installPlan: null,
    keyword: '',
    onlyRedundant: false,
    error: null,
    toast: null,
    log: [],
  }),

  getters: {
    installed: (s): ManagerInfo[] => s.managers.filter((m) => m.detected),

    /** 未安装的管理器（用于下载引导） */
    missing: (s): ManagerInfo[] => s.managers.filter((m) => !m.detected),

    /**
     * 侧边栏用的管理器列表：已安装在前，未安装在后，同组按名称排序。
     * 不再按「期数」分组 —— 阶段标签已从界面移除。
     */
    sidebarManagers(s): ManagerInfo[] {
      return [...s.managers].sort(
        (a, b) => Number(b.detected) - Number(a.detected) || a.name.localeCompare(b.name),
      )
    },

    /** 当前选中的管理器对象 */
    activeManagerInfo(s): ManagerInfo | null {
      return s.activeManager ? s.managers.find((m) => m.id === s.activeManager) ?? null : null
    },

    /**
     * 全局搜索结果：同时命中「包管理器」与「已安装包」。
     *
     * 匹配范围刻意放宽：
     * - 包管理器：名称 / id / 生态
     * - 已安装包：名称 / 版本 / **描述（显示名）** / 路径
     *
     * 最后一项是关键修正：winget、cargo、dotnet 等生态把用户可读的名称放在
     * description 里，而 name 是机器 ID。只匹配 name 会导致「按显示名搜不到包」——
     * 例如 winget 的 Oh My Posh，name 是 `JanDeDobbeleer.OhMyPosh`，
     * 用户输入 `oh my posh` / `ohmyposh` 时此前完全无结果。
     * 归一化（忽略空格、连字符、点）进一步覆盖用户的随手写法。
     *
     * 上限 50 条：结果只是给用户跳转用的，不需要列出全部 500 个包。
     */
    searchHits(s): SearchHit[] {
      const keyword = s.keyword.trim()
      if (!keyword) return []
      const hits: SearchHit[] = []

      // 1) 包管理器
      for (const manager of s.managers) {
        if (matchesKeyword([manager.name, manager.id, manager.language], keyword)) {
          hits.push({
            kind: 'manager',
            managerId: manager.id,
            managerName: manager.name,
            label: manager.name,
            detail: manager.detected
              ? `${shortVersion(manager.version, manager.id)} · ${manager.globalRoot ?? ''}`.trim()
              : '',
          })
        }
      }

      // 2) 已安装包
      for (const record of s.report?.packages ?? []) {
        if (hits.length >= 50) break
        if (matchesKeyword([record.name, record.version, record.description, record.path], keyword)) {
          const managerName = s.managers.find((m) => m.id === record.manager)?.name ?? record.manager
          hits.push({
            kind: 'package',
            managerId: record.manager,
            managerName,
            label: record.name,
            // 有显示名时一并展示，避免用户看到机器 ID 认不出来
            detail: [record.version, record.description, managerName]
              .filter(Boolean)
              .join(' · '),
            record,
          })
        }
      }
      return hits
    },

    /** 某个管理器扫到的包数量 */
    packageCountOf: (s) => (managerId: string) =>
      (s.report?.packages ?? []).filter((p) => p.manager === managerId).length,

    /** 某个管理器的缓存占用 */
    cacheOf: (s) => (managerId: string) =>
      s.caches.find((c) => c.managerId === managerId)?.totalBytes ?? null,

    /** 当前视图的包列表（同样把显示名 description 纳入匹配） */
    visiblePackages(s): PackageRecord[] {
      const all = s.report?.packages ?? []
      return all.filter((p) => {
        if (s.activeManager && p.manager !== s.activeManager) return false
        if (s.onlyRedundant && !p.redundant) return false
        return matchesKeyword([p.name, p.version, p.description, p.path], s.keyword)
      })
    },

    visibleCaches(s): CacheStats[] {
      if (!s.activeManager) return s.caches
      return s.caches.filter((c) => c.managerId === s.activeManager)
    },

    selectableCandidates: (s): CleanCandidate[] => s.candidates.filter((c) => !c.protected),

    /** 当前选中管理器的镜像源 */
    activeRegistry(s) {
      if (!s.activeManager) return null
      return s.managers.find((m) => m.id === s.activeManager)?.registry ?? null
    },

    measuredBytes: (s): number =>
      (s.report?.packages ?? []).reduce((sum, p) => sum + (p.size ?? 0), 0),

    redundantCount: (s): number => (s.report?.packages ?? []).filter((p) => p.redundant).length,
  },

  actions: {
    // ---------------------------------------------------------------- 内部工具
    pushLog(message: string) {
      const stamp = new Date().toLocaleTimeString('zh-CN', { hour12: false })
      this.log.unshift(`[${stamp}] ${message}`)
      if (this.log.length > 200) this.log.pop()
    },

    notify(kind: 'info' | 'success' | 'warn' | 'error', message: string) {
      this.toast = { kind, message }
    },

    handleError(e: unknown, context: string) {
      const err = e instanceof IpcError ? e : IpcError.from(e)
      this.pushLog(`${context} 失败: ${err.message}`)
      if (err.isExpected) {
        this.notify('warn', `${context}：${err.message}`)
        return
      }
      this.error = { code: err.code, message: `${context}：${err.message}` }
    },

    clearError() {
      this.error = null
    },

    clearToast() {
      this.toast = null
    },

    setView(view: ViewKey) {
      this.view = view
      // 离开管理器详情页时清掉选中，避免下次回来看到上一次的分页
      if (view !== 'manager') this.activeManager = null
    },

    /**
     * 进入某个包管理器的详情页。
     *
     * 默认落在「概览」分页 —— 用户先看到它是什么、装在哪、版本多少，
     * 再从分页进入包列表 / 浏览安装 / 管理操作。
     */
    openManager(id: string, tab: ManagerTab = 'overview') {
      this.activeManager = id
      this.managerTab = tab
      this.view = 'manager'
      this.keyword = ''
      this.onlyRedundant = false
      // 浏览结果属于上一个管理器，切换时清空
      this.remotePackages = []
      this.browseQuery = ''
      this.browseError = null
      this.installPlan = null
    },

    setManagerTab(tab: ManagerTab) {
      this.managerTab = tab
    },

    // ---------------------------------------------------------------- 在线浏览
    /**
     * 在包仓库里搜索可安装的包。
     * 只查询官方搜索 API；后端超时/格式变化时返回空列表而不是抛错。
     */
    async browse(query: string) {
      const managerId = this.activeManager
      if (!managerId) return
      if (!query.trim()) {
        this.remotePackages = []
        this.browseError = null
        return
      }
      this.browseLoading = true
      this.browseError = null
      this.browseQuery = query
      this.installPlan = null
      try {
        this.remotePackages = await api.browsePackages({ manager: managerId, query, limit: 25 })
        this.pushLog(`在 ${managerId} 仓库中搜索「${query}」：${this.remotePackages.length} 条结果`)
      } catch (e) {
        const err = e instanceof IpcError ? e : IpcError.from(e)
        this.remotePackages = []
        // 不支持在线浏览属于可预期状态，不弹全局错误条
        this.browseError =
          err.code === 'BROWSE_UNSUPPORTED' || err.code === 'NO_CURL'
            ? err.message
            : `查询失败：${err.message}`
      } finally {
        this.browseLoading = false
      }
    },

    /** 生成安装方案（只返回命令，不执行） */
    async planInstall(packageName: string) {
      const managerId = this.activeManager
      if (!managerId) return null
      try {
        this.installPlan = await api.planInstall(managerId, packageName)
        return this.installPlan
      } catch (e) {
        this.handleError(e, '生成安装方案')
        return null
      }
    },

    clearInstallPlan() {
      this.installPlan = null
    },

    setActiveManager(id: string | null) {
      this.activeManager = id
      this.keyword = ''
      this.onlyRedundant = false
    },

    // ---------------------------------------------------------------- 数据加载
    /** 首屏：先渲染静态定义，再后台探测 */
    async boot(scanOnStartup: boolean, theme = 'dark') {
      this.booting = true
      this.error = null
      try {
        this.supported = await api.supportedManagers()
        this.pushLog(`已加载 ${this.supported.length} 个包管理器定义`)
      } catch (e) {
        this.handleError(e, '加载管理器定义')
      } finally {
        this.booting = false
      }
      await this.detect(false, theme)
      if (scanOnStartup && this.installed.length > 0) {
        // 启动即扫描 → 默认落在「包管理」页时列表就已经有数据
        await this.scan({ measureSize: false, silent: true })
      }
    },

    /**
     * 切换主题后刷新 logo 配色。
     * 只重取 logo，不重新探测（版本号没变，没必要再跑一遍命令）。
     */
    async refreshLogos(theme: string) {
      try {
        const logos = await api.managerLogos(theme)
        for (const item of logos) {
          const manager = this.managers.find((m) => m.id === item.managerId)
          if (manager) manager.logo = item.dataUri
        }
      } catch (e) {
        this.handleError(e, '刷新包管理器 logo')
      }
    },

    /**
     * 按需取 logo（供 ManagerLogo 组件在 store 里没有时单独调用）。
     * 不写入 store.managers，避免改变探测结果的语义。
     */
    async loadLogos(theme: string, managers: string[]) {
      return api.managerLogos(theme, managers)
    },

    /**
     * 探测本机包管理器。
     * `theme` 决定 logo 配色，因此必须传当前生效的主题（dark/light）。
     */
    async detect(force = false, theme = 'dark') {
      this.detecting = true
      try {
        this.managers = await api.detectManagers(force, undefined, theme)
        const found = this.installed.map((m) => `${m.name}${m.version ? ` ${m.version}` : ''}`)
        this.pushLog(
          found.length ? `探测完成，检测到：${found.join('、')}` : '探测完成，未检测到任何包管理器',
        )
        // 下载引导：未检测到的管理器给出官网入口
        try {
          this.hints = await api.installHints()
        } catch {
          this.hints = []
        }
        if (force) {
          this.candidates = []
          this.cleanPhase = 'idle'
          this.pluginsCache = {}
          this.actionsCache = {}
        }
      } catch (e) {
        this.handleError(e, '探测包管理器')
      } finally {
        this.detecting = false
      }
    },

    /**
     * 执行扫描。`measureSize` 会逐个目录统计体积，明显更慢，因此作为可选开关。
     * `silent` 用于启动自动扫描：不弹成功提示，避免打扰。
     */
    async scan(options: { managers?: string[]; measureSize?: boolean; silent?: boolean } = {}) {
      this.scanning = true
      this.error = null
      try {
        const report = await api.runScan({
          managers: options.managers ?? (this.activeManager ? [this.activeManager] : []),
          measurePackageSize: options.measureSize ?? false,
          timeoutMs: 30_000,
        })
        this.report = report
        this.caches = report.caches
        // 扫描结果与旧候选不再对应，强制重置两阶段状态
        this.candidates = []
        this.cleanPhase = 'idle'
        this.previewResults = []
        this.pluginsCache = {}
        this.pushLog(
          `扫描完成：${report.totalPackages} 个包，缓存 ${report.totalCacheBytes} 字节，耗时 ${report.durationMs} ms`,
        )
        if (!options.silent) {
          this.notify(
            'success',
            options.measureSize
              ? `扫描完成，共 ${report.totalPackages} 个包（含体积统计）`
              : `扫描完成，共 ${report.totalPackages} 个包`,
          )
        }
      } catch (e) {
        this.handleError(e, '扫描')
      } finally {
        this.scanning = false
      }
    },

    /** 某个包是否已经扫过（用于管理页按钮状态） */
    hasScanned(managerId: string): boolean {
      return (this.report?.packages ?? []).some((p) => p.manager === managerId)
    },

    /** 只刷新当前管理器的缓存统计（比整轮扫描快得多） */
    async refreshCacheStats(managerId: string) {
      try {
        const stat = await api.getCacheStats(managerId)
        const idx = this.caches.findIndex((c) => c.managerId === managerId)
        if (idx >= 0) this.caches[idx] = stat
        else this.caches.push(stat)
      } catch (e) {
        this.handleError(e, `读取 ${managerId} 缓存统计`)
      }
    },

    async loadRegistry(managerId: string) {
      try {
        const cfg = await api.getRegistry(managerId)
        const target = this.managers.find((m) => m.id === managerId)
        if (target) target.registry = cfg
        return cfg
      } catch (e) {
        this.handleError(e, `读取 ${managerId} 镜像源`)
        return null
      }
    },

    async saveRegistry(managerId: string, key: string, value: string) {
      this.savingRegistry = true
      try {
        const message = await api.setRegistry(managerId, key, value)
        this.pushLog(message)
        this.notify('success', message)
        await this.loadRegistry(managerId)
        return true
      } catch (e) {
        this.handleError(e, '写入镜像源')
        return false
      } finally {
        this.savingRegistry = false
      }
    },

    // ---------------------------------------------------------------- 包管理交互
    /** 右键菜单的动作列表（带缓存） */
    async loadActions(record: PackageRecord): Promise<ManagementAction[]> {
      const key = `${record.manager}\u{1}${record.name}`
      const cached = this.actionsCache[key]
      if (cached) return cached
      try {
        const actions = await api.packageActions(record.manager, record.name, record.scope)
        this.actionsCache[key] = actions
        return actions
      } catch (e) {
        this.handleError(e, '读取管理动作')
        return []
      }
    },

    /** 展开包内插件 / 依赖（按需加载） */
    async loadPlugins(record: PackageRecord, force = false): Promise<PluginNode[]> {
      const key = `${record.manager}\u{1}${record.name}`
      if (!force && this.pluginsCache[key]) {
        record.plugins = this.pluginsCache[key]
        record.pluginsLoaded = true
        return this.pluginsCache[key]
      }
      this.pluginsLoading = key
      try {
        const nodes = await api.packagePlugins({
          manager: record.manager,
          package: record.name,
          version: record.version,
          path: record.path,
          timeoutMs: 25_000,
        })
        this.pluginsCache[key] = nodes
        record.plugins = nodes
        record.pluginsLoaded = true
        return nodes
      } catch (e) {
        this.handleError(e, '分析包内容')
        return []
      } finally {
        this.pluginsLoading = null
      }
    },

    /** 打开外部链接：kind = manager 时由后端决定地址 */
    async openManagedLink(kind: 'manager' | 'docs', managerId: string) {
      try {
        await api.openExternalLink({ kind, target: managerId })
      } catch (e) {
        this.handleError(e, '打开链接')
      }
    },

    async openUrl(url: string) {
      try {
        await api.openExternalLink({ kind: 'url', target: url })
      } catch (e) {
        this.handleError(e, '打开链接')
      }
    },

    // ---------------------------------------------------------------- 清理流程
    async loadCandidates() {
      this.loadingCandidates = true
      this.error = null
      try {
        this.candidates = await api.listCleanCandidates(30_000)
        this.cleanPhase = 'idle'
        this.previewResults = []
        this.pushLog(`发现 ${this.candidates.length} 个可清理项`)
      } catch (e) {
        this.handleError(e, '枚举清理候选')
      } finally {
        this.loadingCandidates = false
      }
    },

    async previewClean(ids: string[]) {
      if (ids.length === 0) {
        this.notify('warn', '请先勾选要清理的项目')
        return
      }
      this.cleanPhase = 'previewing'
      try {
        this.previewResults = await api.cleanCaches({ candidateIds: ids, dryRun: true }, 30_000)
        this.cleanPhase = 'previewed'
      } catch (e) {
        this.cleanPhase = 'idle'
        this.handleError(e, '预览清理')
      }
    },

    previewTotalBytes(): number {
      return this.previewResults.filter((r) => r.ok).reduce((sum, r) => sum + r.freedBytes, 0)
    },

    async executeClean(ids: string[]) {
      this.cleanPhase = 'executing'
      try {
        const results = await api.cleanCaches({ candidateIds: ids, dryRun: false }, 60_000)
        this.previewResults = results
        this.cleanPhase = 'done'
        const freed = results.filter((r) => r.ok).reduce((sum, r) => sum + r.freedBytes, 0)
        const failed = results.filter((r) => !r.ok)
        this.pushLog(`清理完成：释放 ${freed} 字节，失败 ${failed.length} 项`)
        if (failed.length) {
          this.notify('warn', `清理完成，${failed.length} 项未能删除（文件可能被占用）`)
        } else {
          this.notify('success', '清理完成')
        }
        await this.loadCandidates()
        return results
      } catch (e) {
        this.cleanPhase = 'idle'
        this.handleError(e, '执行清理')
        return []
      }
    },

    resetClean() {
      this.cleanPhase = 'idle'
      this.previewResults = []
    },

    // ---------------------------------------------------------------- 导出
    async exportReport(format: ExportFormat, targetPath: string) {
      if (!this.report) {
        this.notify('warn', '请先执行一次扫描')
        return false
      }
      try {
        const saved = await api.exportReport(this.report, { format, targetPath })
        this.pushLog(`报告已导出：${saved}`)
        this.notify('success', `报告已保存到 ${saved}`)
        return true
      } catch (e) {
        this.handleError(e, '导出报告')
        return false
      }
    },
  },
})
