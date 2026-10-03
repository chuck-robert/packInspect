/**
 * 全局状态。单一 store 足够：数据量不大，且各视图之间强耦合（扫描结果 → 表格 / 缓存 / 清理）。
 */

import { defineStore } from 'pinia'
import { api, IpcError, type SupportedManager } from '@/api'
import type {
  CacheStats,
  CleanCandidate,
  CleanPhase,
  CleanResult,
  ExportFormat,
  ManagerInfo,
  PackageRecord,
  ScanReport,
} from '@/types'

interface State {
  /** 静态定义（首屏骨架） */
  supported: SupportedManager[]
  /** 探测结果 */
  managers: ManagerInfo[]
  report: ScanReport | null
  caches: CacheStats[]
  candidates: CleanCandidate[]
  /** cleanup 两阶段状态机 */
  cleanPhase: CleanPhase
  previewResults: CleanResult[]

  booting: boolean
  detecting: boolean
  scanning: boolean
  loadingCandidates: boolean
  savingRegistry: boolean

  /** 当前选中的管理器（null = 全部） */
  activeManager: string | null
  /** 关键字搜索 */
  keyword: string
  /** 是否只看冗余项 */
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
    cleanPhase: 'idle',
    previewResults: [],
    booting: false,
    detecting: false,
    scanning: false,
    loadingCandidates: false,
    savingRegistry: false,
    activeManager: null,
    keyword: '',
    onlyRedundant: false,
    error: null,
    toast: null,
    log: [],
  }),

  getters: {
    /** 已安装的管理器 */
    installed: (s): ManagerInfo[] => s.managers.filter((m) => m.detected),

    /** 侧边栏分组：按语言生态归并 */
    groupedManagers(s): { language: string; items: ManagerInfo[] }[] {
      const byLang = new Map<string, ManagerInfo[]>()
      for (const m of s.managers) {
        if (!byLang.has(m.language)) byLang.set(m.language, [])
        byLang.get(m.language)!.push(m)
      }
      return [...byLang.entries()].map(([language, items]) => ({ language, items }))
    },

    /** 当前视图的包列表 */
    visiblePackages(s): PackageRecord[] {
      const all = s.report?.packages ?? []
      const kw = s.keyword.trim().toLowerCase()
      return all.filter((p) => {
        if (s.activeManager && p.manager !== s.activeManager) return false
        if (s.onlyRedundant && !p.redundant) return false
        if (!kw) return true
        return (
          p.name.toLowerCase().includes(kw) ||
          (p.version ?? '').toLowerCase().includes(kw) ||
          (p.path ?? '').toLowerCase().includes(kw)
        )
      })
    },

    /** 当前视图的缓存统计 */
    visibleCaches(s): CacheStats[] {
      if (!s.activeManager) return s.caches
      return s.caches.filter((c) => c.managerId === s.activeManager)
    },

    /** 可清理候选（受保护项不可勾选） */
    selectableCandidates: (s): CleanCandidate[] => s.candidates.filter((c) => !c.protected),

    /** 当前选中管理器对应的镜像源 */
    activeRegistry(s) {
      if (!s.activeManager) return null
      return s.managers.find((m) => m.id === s.activeManager)?.registry ?? null
    },

    /** 全部包体积合计（仅在测量过体积时有意义） */
    measuredBytes: (s): number =>
      (s.report?.packages ?? []).reduce((sum, p) => sum + (p.size ?? 0), 0),
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

    /** 统一错误处理：可预期状态（未安装等）不弹红条 */
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

    setActiveManager(id: string | null) {
      this.activeManager = id
      this.keyword = ''
      this.onlyRedundant = false
    },

    // ---------------------------------------------------------------- 数据加载
    /** 首屏：先渲染静态定义，再后台探测 */
    async boot() {
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
      await this.detect()
    },

    async detect(force = false) {
      this.detecting = true
      try {
        this.managers = await api.detectManagers(force)
        const found = this.installed.map((m) => `${m.name}${m.version ? ` ${m.version}` : ''}`)
        this.pushLog(
          found.length ? `探测完成，检测到：${found.join('、')}` : '探测完成，未检测到任何包管理器',
        )
        // 探测结果变化后，旧的扫描结果可能失效
        if (force) {
          this.candidates = []
          this.cleanPhase = 'idle'
        }
      } catch (e) {
        this.handleError(e, '探测包管理器')
      } finally {
        this.detecting = false
      }
    },

    /**
     * 执行扫描。`measureSize` 会逐个目录统计体积，明显更慢，因此作为可选开关。
     */
    async scan(options: { managers?: string[]; measureSize?: boolean } = {}) {
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
        this.pushLog(
          `扫描完成：${report.totalPackages} 个包，缓存 ${report.totalCacheBytes} 字节，耗时 ${report.durationMs} ms`,
        )
        this.notify(
          'success',
          options.measureSize
            ? `扫描完成，共 ${report.totalPackages} 个包（含体积统计）`
            : `扫描完成，共 ${report.totalPackages} 个包`,
        )
      } catch (e) {
        this.handleError(e, '扫描')
      } finally {
        this.scanning = false
      }
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

    /** 保存镜像源：先预览，再落盘（后端会自动备份原文件） */
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

    // ---------------------------------------------------------------- 清理流程
    /** 第一阶段：枚举候选 */
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

    /**
     * 第二阶段：dry-run 预览。后端只算账不删除，
     * 返回的 freedBytes 即「确认后能释放多少」。
     */
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

    /** 第三阶段：真正执行（调用方必须已获得用户二次确认） */
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

    /** 取消预览，回到未选中状态 */
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
