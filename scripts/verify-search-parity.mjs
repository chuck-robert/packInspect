/**
 * 验证「包列表分页的搜索」与「顶部搜索」使用同一套匹配规则。
 *
 * 背景：两处曾用不同规则 ——
 *   顶部搜索: matchesKeyword（归一化 + name/version/description/path）
 *   包列表分页: 普通 includes（只查 name/version）
 * 规则不一致会出现「顶部搜得到、这里搜不到」。本脚本用真实 winget 记录做对照。
 *
 * 同时校验源码确实已统一，避免以后有人改回去。
 */
import { readFileSync } from 'node:fs'

const panel = readFileSync(new URL('../src/components/ManagerPackagesPanel.vue', import.meta.url), 'utf8')

// ---- 校验源码：分页搜索必须用 matchesKeyword，且不能再出现旧写法 ----
const usesMatchesKeyword = /matchesKeyword\(\[p\.name, p\.version, p\.description, p\.path\]/.test(panel)
const hasOldPlainIncludes = /p\.name\.toLowerCase\(\)\.includes\(keyword\)/.test(panel)

console.log('=== 源码校验 ===')
console.log('  包列表分页使用 matchesKeyword:', usesMatchesKeyword ? '✅' : '❌')
console.log('  已移除旧的普通 includes 写法:', hasOldPlainIncludes ? '❌ 仍存在' : '✅')

// ---- 复刻两种实现 ----
const normalizeForSearch = (s) => s.toLowerCase().replace(/[\s\-_.]+/g, '')
const matchesKeyword = (fields, keyword) => {
  const needle = normalizeForSearch(keyword)
  if (!needle) return true
  return fields.some((f) => !!f && normalizeForSearch(f).includes(needle))
}
const oldPanelMatch = (record, keyword) => {
  const k = keyword.trim().toLowerCase()
  if (!k) return true
  return record.name.toLowerCase().includes(k) || (record.version ?? '').toLowerCase().includes(k)
}

// ---- 真实的 winget 记录（winget list 解析结果：Id 作 name，Name 入 description）----
const ohMyPosh = {
  manager: 'winget',
  name: 'JanDeDobbeleer.OhMyPosh',
  version: '31.4.0',
  description: 'Oh My Posh',
  path: null,
}

console.log('\n=== 记录 ===')
console.log(' ', JSON.stringify(ohMyPosh))

const keywords = ['ohmyposh', 'oh my posh', 'Oh-My-Posh', 'OhMyPosh', 'posh', 'JanDeDobbeleer']
console.log('\n=== 各关键字的匹配结果 ===')
console.log('  关键字'.padEnd(22) + '旧(分页 includes)'.padEnd(20) + '新(统一 matchesKeyword)')
let diverge = 0
for (const kw of keywords) {
  const oldR = oldPanelMatch(ohMyPosh, kw)
  const newR = matchesKeyword(
    [ohMyPosh.name, ohMyPosh.version, ohMyPosh.description, ohMyPosh.path],
    kw,
  )
  const mark = oldR === newR ? '  ' : '≠ '
  if (oldR !== newR) diverge++
  console.log(`  ${mark}${kw.padEnd(20)}${String(oldR).padEnd(20)}${newR}`)
}
console.log(`\n  两种规则结果不一致的关键字数量: ${diverge}`)

// ---- 断言 ----
let failed = 0
const assert = (cond, msg) => {
  console.log(`  ${cond ? '✅' : '❌'} ${msg}`)
  if (!cond) failed++
}
console.log('\n=== 断言 ===')
assert(usesMatchesKeyword, '源码已改用 matchesKeyword')
assert(!hasOldPlainIncludes, '源码不再有旧的普通 includes 写法')
assert(
  matchesKeyword([ohMyPosh.name, ohMyPosh.version, ohMyPosh.description], 'oh my posh'),
  '带空格的 "oh my posh" 现在能命中（旧实现不命中）',
)
assert(
  matchesKeyword([ohMyPosh.name, ohMyPosh.version, ohMyPosh.description], 'ohmyposh'),
  '"ohmyposh" 能命中',
)
assert(
  matchesKeyword([ohMyPosh.name, ohMyPosh.version, ohMyPosh.description], 'JanDeDobbeleer'),
  '包 ID 片段能命中',
)

console.log(failed === 0 ? '\n全部通过 ✅' : `\n${failed} 项失败 ❌`)
process.exit(failed === 0 ? 0 : 1)
