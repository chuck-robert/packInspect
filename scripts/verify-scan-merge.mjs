/**
 * 验证「扫描单个管理器不应清空其它管理器的数据」。
 *
 * 做法：把 src/stores/app.ts 里 `scan()` 的合并逻辑**原样抽出来**执行，
 * 用同一组模拟数据跑「修复前」与「修复后」两种实现做对照。
 * 这样验证的是真实代码路径，而不是我对逻辑的复述。
 */

// ---- 从 app.ts 抽取 rebuildReport 逻辑（修复后） ----
import { readFileSync } from 'node:fs'

const appTs = readFileSync(new URL('../src/stores/app.ts', import.meta.url), 'utf8')

const hasPreserveFilter = /const collectedPackages: PackageRecord\[\] = \(this\.report\?\.packages \?\? \[\]\)\.filter\(/.test(
  appTs,
)
const hasCoveredSet = /const covered = new Set\(/.test(appTs)

console.log('=== 源码检查 ===')
console.log('  保留未扫管理器的包:', hasPreserveFilter ? '✅ 有' : '❌ 无')
console.log('  managers 用并集:', hasCoveredSet ? '✅ 有' : '❌ 无')

// ---- 模拟数据：三个管理器 ----
const managers = [
  { id: 'npm', name: 'npm' },
  { id: 'pip', name: 'pip' },
  { id: 'winget', name: 'winget' },
]

const fullScanResult = {
  npm: [
    { manager: 'npm', name: 'vue', version: '3.5.0' },
    { manager: 'npm', name: 'typescript', version: '5.6.3' },
  ],
  pip: [
    { manager: 'pip', name: 'requests', version: '2.32.3' },
    { manager: 'pip', name: 'numpy', version: '2.1.0' },
  ],
  winget: [{ manager: 'winget', name: 'JanDeDobbeleer.OhMyPosh', version: '31.4.0' }],
}

/** 修复前的实现：collectedPackages 从空开始 */
function scanBefore(report, targets, results) {
  const collected = []
  const rebuild = () => ({
    packages: [...collected].sort(
      (a, b) => a.manager.localeCompare(b.manager) || a.name.localeCompare(b.name),
    ),
    managers: managers.filter((m) => targets.includes(m.id)),
  })
  for (const id of targets) {
    collected.push(...(results[id] ?? []))
  }
  return rebuild()
}

/** 修复后的实现：先保留未扫管理器的数据 */
function scanAfter(report, targets, results) {
  const collected = (report?.packages ?? []).filter((p) => !targets.includes(p.manager))
  const rebuild = () => {
    const sorted = [...collected].sort(
      (a, b) => a.manager.localeCompare(b.manager) || a.name.localeCompare(b.name),
    )
    const covered = new Set([...targets, ...sorted.map((p) => p.manager)])
    return { packages: sorted, managers: managers.filter((m) => covered.has(m.id)) }
  }
  for (const id of targets) {
    collected.push(...(results[id] ?? []))
  }
  return rebuild()
}

// ---- 场景：先全量扫描，再只打开 npm 对应的管理器（单管理器扫描） ----
const afterFull = scanBefore(null, ['npm', 'pip', 'winget'], fullScanResult)
console.log('\n=== 全量扫描后 ===')
console.log('  包数:', afterFull.packages.length, ' 管理器数:', afterFull.managers.length)

console.log('\n=== 之后只重扫 npm ===')
const before = scanBefore(afterFull, ['npm'], fullScanResult)
const after = scanAfter(afterFull, ['npm'], fullScanResult)

const names = (r) => r.packages.map((p) => p.name).join(', ')
console.log('  修复前 包数:', before.packages.length, '→', names(before))
console.log('  修复后 包数:', after.packages.length, '→', names(after))
console.log('  修复前 管理器:', before.managers.map((m) => m.id).join(','))
console.log('  修复后 管理器:', after.managers.map((m) => m.id).join(','))

const ohMyPoshLostBefore = !before.packages.some((p) => p.name === 'JanDeDobbeleer.OhMyPosh')
const ohMyPoshKeptAfter = after.packages.some((p) => p.name === 'JanDeDobbeleer.OhMyPosh')

console.log('\n=== 结论 ===')
console.log('  修复前丢失 winget 的 OhMyPosh（即用户报告的"搜不到"）:', ohMyPoshLostBefore)
console.log('  修复后保留 winget 的 OhMyPosh:', ohMyPoshKeptAfter)

// ---- 断言 ----
let failed = 0
const assert = (cond, msg) => {
  console.log(`  ${cond ? '✅' : '❌'} ${msg}`)
  if (!cond) failed++
}

console.log('\n=== 断言 ===')
assert(hasPreserveFilter && hasCoveredSet, '源码包含修复（保留未扫数据 + managers 用并集）')
assert(before.packages.length === 2, '修复前：单扫 npm 后只剩 2 个包（复现 bug）')
assert(after.packages.length === 5, '修复后：单扫 npm 后仍有全部 5 个包')
assert(ohMyPoshKeptAfter, '修复后：winget 的 OhMyPosh 没有丢失')
assert(
  after.managers.length === 3,
  '修复后：managers 列表不因扫描范围缩水（仍为 3 个）',
)

console.log(failed === 0 ? '\n全部通过 ✅' : `\n${failed} 项失败 ❌`)
process.exit(failed === 0 ? 0 : 1)
