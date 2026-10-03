/**
 * 功能自检脚本（开发辅助，不属于应用运行时）。
 *
 * 在应用的 DevTools 控制台或 `npm run dev` 页面的控制台里执行：
 *   await import('/scripts/smoke.ts')
 *
 * 它会通过真实 IPC 走一遍核心链路并打印结果，用于快速确认后端各命令是否正常。
 * 之所以做成脚本而不是单元测试：Tauri 的 IPC 只有在 WebView 里才存在。
 */

const out = (label: string, value: unknown) => {
  // eslint-disable-next-line no-console
  console.log(`%c[smoke] ${label}`, 'color:#4c9aff;font-weight:600', value)
}

async function main() {
  const { invoke } = await import('@tauri-apps/api/core')

  // 1. 探测器
  const managers = await invoke<Array<{ id: string; detected: boolean; version: string | null; tier: number }>>(
    'detect_managers',
    { force: true },
  )
  const installed = managers.filter((m) => m.detected)
  out(`探测完成：${installed.length}/${managers.length}`, installed.map((m) => `${m.id}@${m.version}`))

  // 2. 未安装管理器的下载引导
  const hints = await invoke<Array<{ managerId: string; downloadUrl: string | null }>>('install_hints')
  out(`下载引导 ${hints.length} 条`, hints.slice(0, 3))

  // 3. 扫描
  const report = await invoke<{
    totalPackages: number
    totalCacheBytes: number
    packages: Array<{
      name: string
      version: string | null
      manager: string
      scope: string
      icon: string | null
      path: string | null
    }>
  }>('run_scan', { request: { measurePackageSize: false, timeoutMs: 30_000 } })
  out(`扫描：${report.totalPackages} 个包 / 缓存 ${report.totalCacheBytes} 字节`, {
    样例: report.packages.slice(0, 5).map((p) => `${p.manager}:${p.name}@${p.version}`),
    有图标的比例: `${report.packages.filter((p) => p.icon).length}/${report.packages.length}`,
  })

  // 4. 右键管理动作（验证占位按钮语义）
  const sample = report.packages.find((p) => p.manager === 'npm') ?? report.packages[0]
  if (!sample) {
    out('没有扫到任何包，后续检查跳过', null)
    return
  }
  const actions = await invoke<Array<{ action: string; enabled: boolean; destructive: boolean; commandHint: string | null }>>(
    'package_actions',
    { managerId: sample.manager, package: sample.name, scope: sample.scope },
  )
  out(`${sample.manager}:${sample.name} 的管理动作`, actions)
  const destructiveEnabled = actions.filter((a) => a.destructive && a.enabled)
  out(
    destructiveEnabled.length === 0 ? '✅ 破坏性动作全部为占位（不会误执行）' : '❌ 存在可执行的破坏性动作',
    destructiveEnabled,
  )

  // 5. 包内子节点
  const plugins = await invoke<Array<{ nodeType: string; name: string; version: string | null }>>(
    'package_plugins',
    {
      request: {
        manager: sample.manager,
        package: sample.name,
        version: sample.version,
        path: sample.path,
        timeoutMs: 20_000,
      },
    },
  )
  out(`${sample.name} 的包内子节点 ${plugins.length} 条`, plugins.slice(0, 8))

  // 6. 图标命令
  const icon = await invoke<{ key: string; dataUri: string; cached: boolean }>('package_icon', {
    managerId: sample.manager,
    package: sample.name,
  })
  out('图标命令', { key: icon.key, cached: icon.cached, 前缀: icon.dataUri.slice(0, 32) })

  // 7. 设置往返
  const before = await invoke<Record<string, unknown>>('get_settings')
  out('当前设置', before)

  // 8. 链接白名单（应被拒绝）
  try {
    await invoke('open_external_link', { request: { kind: 'url', target: 'https://evil.example.com/x' } })
    out('❌ 危险链接未被拦截', null)
  } catch (e) {
    out('✅ 危险链接被拒绝', e)
  }

  const diag = await invoke<Record<string, unknown>>('get_diagnostics')
  out('诊断：PATH 目录数', diag.pathDirCount)
  out('诊断：各管理器解析结果', diag.probes)
}

main().catch((e) => out('脚本出错', e))
