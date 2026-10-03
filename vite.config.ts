import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

// Tauri 要求固定端口 + 不自动切换，否则 Rust 侧 devUrl 会对不上
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri 由 cargo 自己监视，Vite 不要跟着重启
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'chrome110',
    minify: 'esbuild',
    sourcemap: false,
  },
})
