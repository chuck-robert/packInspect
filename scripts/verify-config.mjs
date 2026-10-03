/**
 * 校验 JSON 配置文件**不带 UTF-8 BOM**。
 *
 * 为什么需要这条检查（真实踩过的坑）：
 * PowerShell 的 `Set-Content -Encoding utf8`（Windows PowerShell 5.1 与
 * PowerShell 7 的默认行为）会写入 **UTF-8 BOM**（EF BB BF）。
 * 带 BOM 的 JSON 虽然被很多解析器容忍，但 Rust 的 serde_json / Tauri 的配置解析器
 * **不容忍** —— 报错信息还极其误导：
 *
 *     unable to parse JSON Tauri config file ... because expected value at line 1 column 1
 *
 * 「line 1 column 1」看起来像文件为空或语法错误，实际是开头那 3 个字节。
 * 我用脚本改 tauri.conf.json 时就这样把构建搞坏过一轮。
 *
 * 本脚本同时覆盖：
 * - 所有 JSON 文件不得有 BOM
 * - tauri.conf.json 必须能被严格解析
 * - 打包用的 resources 路径必须真实存在（否则装完少了文件，功能静默失效）
 */
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))

/** 递归找出所有 .json（跳过 node_modules / dist / target / .logs） */
function findJson(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (['node_modules', 'dist', 'target', '.git', '.logs', 'gen'].includes(name)) continue
    const p = join(dir, name)
    const st = statSync(p)
    if (st.isDirectory()) findJson(p, out)
    else if (name.endsWith('.json')) out.push(p)
  }
  return out
}

const files = findJson(root)
let failed = 0

console.log('=== JSON BOM 检查 ===')
const withBom = []
for (const f of files) {
  const buf = readFileSync(f)
  if (buf.length >= 3 && buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf) {
    withBom.push(relative(root, f))
  }
}
if (withBom.length === 0) {
  console.log(`  ✅ 检查了 ${files.length} 个 JSON 文件，均无 BOM`)
} else {
  console.log(`  ❌ ${withBom.length} 个文件带 UTF-8 BOM（会导致解析失败）:`)
  for (const f of withBom) console.log(`     - ${f}`)
  failed++
}

console.log('\n=== tauri.conf.json 结构检查 ===')
const confPath = join(root, 'src-tauri', 'tauri.conf.json')
let conf = null
try {
  conf = JSON.parse(readFileSync(confPath, 'utf8'))
  console.log('  ✅ 可被严格解析')
} catch (e) {
  console.log(`  ❌ 解析失败: ${e.message}`)
  failed++
}

if (conf) {
  // resources 里列的文件必须真实存在，否则装完会缺文件、功能静默失效
  const resources = conf.bundle?.resources ?? {}
  const entries = Object.entries(resources)
  if (entries.length === 0) {
    console.log('  ⚠️  bundle.resources 为空 —— 若有运行时需要的脚本，安装版会缺少它们')
  } else {
    for (const [src] of entries) {
      const abs = join(root, 'src-tauri', src)
      const ok = existsSync(abs)
      console.log(`  ${ok ? '✅' : '❌'} resource 存在: ${src}`)
      if (!ok) failed++
    }
  }

  // 运行时会去 <安装目录>/scripts/ 找这些脚本，必须与 package_ops/console 的查找路径一致
  const expected = [
    'scripts/run-install.ps1',
  ]
  for (const want of expected) {
    const listed = entries.some(([, dest]) => dest === want || dest.endsWith(want))
    console.log(`  ${listed ? '✅' : '❌'} 已打包运行时脚本: ${want}`)
    if (!listed) failed++
  }

  // 图标文件必须真实存在，否则打包会失败或退回默认图标
  const icons = conf.bundle?.icon ?? []
  if (icons.length === 0) {
    console.log('  ❌ bundle.icon 为空')
    failed++
  }
  for (const rel of icons) {
    const abs = join(root, 'src-tauri', rel)
    const ok = existsSync(abs)
    console.log(`  ${ok ? '✅' : '❌'} 图标存在: ${rel}`)
    if (!ok) failed++
  }

  // 安装程序图标另有一份配置，漏了会退回 Tauri 默认图标
  const nsis = conf.bundle?.windows?.nsis ?? {}
  for (const key of ['installerIcon', 'uninstallerIcon']) {
    if (nsis[key]) {
      const abs = join(root, 'src-tauri', nsis[key])
      const ok = existsSync(abs)
      console.log(`  ${ok ? '✅' : '❌'} ${key} 存在: ${nsis[key]}`)
      if (!ok) failed++
    }
  }

  // 向导相关字段名容易写成 Tauri v1 的（oneClick 等），这里直接校验未知字段
  const allowedNsis = new Set([
    'template', 'headerImage', 'sidebarImage', 'installerIcon', 'uninstallerIcon',
    'uninstallerHeaderImage', 'installMode', 'languages', 'customLanguageFiles',
    'displayLanguageSelector', 'compression', 'startMenuFolder', 'installerHooks',
    'minimumWebview2Version',
  ])
  const unknown = Object.keys(nsis).filter((k) => !allowedNsis.has(k))
  if (unknown.length > 0) {
    console.log(`  ❌ nsis 里有 Tauri v2 不认的字段（很可能是 v1 的写法）: ${unknown.join(', ')}`)
    failed++
  } else {
    console.log('  ✅ nsis 字段全部为 Tauri v2 认可的键')
  }
}

console.log(failed === 0 ? '\n全部通过 ✅' : `\n${failed} 项失败 ❌`)
process.exit(failed === 0 ? 0 : 1)
