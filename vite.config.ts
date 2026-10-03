import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'
import { readFileSync } from 'node:fs'

// 版本号从 package.json 读一次注入前端，避免「设置页显示的版本」与发布版本不一致
const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf-8')) as {
  version: string
}

// Tauri 要求固定端口 + 不自动切换，否则 Rust 侧 devUrl 会对不上
export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    // 显式绑定 IPv4：默认的 `localhost` 在部分 Windows 上只解析到 ::1，
    // 会导致外部探测（启动脚本 / Tauri）连 127.0.0.1 时被拒绝。
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri 由 cargo 自己监视，Vite 不要跟着重启。
      // .logs 必须忽略：cargo 会把完整输出实时写入 .logs/build.log，
      // 该文件在构建期间被占用，Vite 去 watch 会直接抛 EBUSY 崩溃。
      ignored: ['**/src-tauri/**', '**/.logs/**'],
    },
  },
  build: {
    target: 'chrome110',
    minify: 'esbuild',
    sourcemap: false,
  },
})
