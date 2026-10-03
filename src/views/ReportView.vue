<script setup lang="ts">
/**
 * 报告与日志视图：导出扫描报告、查看系统诊断、查看操作日志。
 * 「另存为」使用 Tauri 的 dialog 插件（只负责选路径），写文件仍由 Rust 侧完成。
 */
import { computed, onMounted, ref } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { useAppStore } from '@/stores/app'
import { api, IpcError } from '@/api'
import type { ExportFormat } from '@/types'
import { formatBytes, formatCount, formatDateTime, formatDuration } from '@/utils/format'

const store = useAppStore()
const diagnostics = ref<Record<string, unknown> | null>(null)
const diagError = ref('')
const exporting = ref<ExportFormat | null>(null)

const FORMATS: { key: ExportFormat; label: string; ext: string; desc: string }[] = [
  { key: 'json', label: 'JSON', ext: 'json', desc: '完整结构化数据，适合二次处理' },
  { key: 'csv', label: 'CSV', ext: 'csv', desc: '包列表与缓存统计，适合 Excel 打开' },
  { key: 'markdown', label: 'Markdown', ext: 'md', desc: '表格化报告，适合贴进文档或工单' },
]

const summary = computed(() => store.report)

onMounted(async () => {
  try {
    diagnostics.value = await api.diagnostics()
  } catch (e) {
    diagError.value = e instanceof IpcError ? e.message : String(e)
  }
})

async function doExport(format: ExportFormat, ext: string) {
  if (!store.report) {
    store.notify('warn', '请先执行一次扫描再导出')
    return
  }
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-')
  const targetPath = await save({
    title: '导出扫描报告',
    defaultPath: `packinspect-report-${stamp}.${ext}`,
    filters: [{ name: format.toUpperCase(), extensions: [ext] }],
  })
  if (!targetPath) return // 用户取消
  exporting.value = format
  try {
    await store.exportReport(format, targetPath)
  } finally {
    exporting.value = null
  }
}
</script>

<template>
  <section class="scroll-area" style="padding: 12px 14px">
    <div class="col">
      <!-- 概览 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">扫描概览</span>
          <span class="panel__spacer" />
          <span class="panel__sub">{{ formatDateTime(summary?.generatedAt) }}</span>
        </div>
        <div class="panel__body">
          <div v-if="!summary" class="hint">还没有扫描结果。</div>
          <div v-else class="stat-grid">
            <div class="stat">
              <div class="stat__label">包总数</div>
              <div class="stat__value">{{ formatCount(summary.totalPackages) }}</div>
            </div>
            <div class="stat">
              <div class="stat__label">缓存总占用</div>
              <div class="stat__value">{{ formatBytes(summary.totalCacheBytes) }}</div>
            </div>
            <div class="stat">
              <div class="stat__label">扫描耗时</div>
              <div class="stat__value">{{ formatDuration(summary.durationMs) }}</div>
            </div>
            <div class="stat">
              <div class="stat__label">冗余项</div>
              <div class="stat__value">{{ summary.packages.filter((p) => p.redundant).length }}</div>
            </div>
          </div>
        </div>
      </div>

      <!-- 导出 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">导出报告</span>
        </div>
        <div class="panel__body col">
          <div class="row" style="flex-wrap: wrap">
            <button
              v-for="f in FORMATS"
              :key="f.key"
              class="btn"
              :disabled="!summary || exporting !== null"
              @click="doExport(f.key, f.ext)"
            >
              <span v-if="exporting === f.key" class="spinner" />
              导出 {{ f.label }}
            </button>
          </div>
          <div class="hint">
            JSON = 完整数据；CSV = 包列表 + 缓存统计；Markdown = 可直接阅读的表格报告。
            导出路径由你选择，文件由后端写入。
          </div>
        </div>
      </div>

      <!-- 诊断 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">运行环境</span>
        </div>
        <div class="panel__body">
          <div v-if="diagError" class="banner banner--error">{{ diagError }}</div>
          <div v-else-if="!diagnostics" class="hint">读取中…</div>
          <div v-else class="kv">
            <span class="kv__k">操作系统</span>
            <span class="kv__v">{{ diagnostics.os }} / {{ diagnostics.arch }}</span>
            <span class="kv__k">用户主目录</span>
            <span class="kv__v">{{ diagnostics.home ?? '未识别' }}</span>
            <span class="kv__k">支持的管理器</span>
            <span class="kv__v">
              {{ (diagnostics.supportedManagers as string[])?.join('、') }}
            </span>
          </div>
          <p class="hint" style="margin-top: 10px">
            安全模型：前端不具备执行 shell 与读取文件系统的能力。所有命令都由 Rust 侧按
            <strong>白名单</strong>执行，参数为静态数组；清理操作只允许作用于已识别的缓存目录，且必须先经过
            dry-run 预览与二次确认。
          </p>
        </div>
      </div>

      <!-- 日志 -->
      <div class="panel">
        <div class="panel__head">
          <span class="panel__title">操作日志</span>
          <span class="panel__spacer" />
          <button
            class="btn btn--ghost btn--sm"
            @click="
              () => {
                store.log.splice(0)
              }
            "
          >
            清空
          </button>
        </div>
        <div class="panel__body">
          <pre v-if="store.log.length" class="diff" style="max-height: 280px">{{
            store.log.join('\n')
          }}</pre>
          <div v-else class="hint">暂无日志。</div>
        </div>
      </div>
    </div>
  </section>
</template>
