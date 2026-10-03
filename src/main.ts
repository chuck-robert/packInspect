import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import './styles/theme.css'

const app = createApp(App)
app.use(createPinia())

// 挂载前先落一次主题，避免深色应用闪一下浅色
try {
  const raw = localStorage.getItem('packinspect.theme')
  if (raw === 'light' || raw === 'dark') {
    document.documentElement.dataset.theme = raw
    document.documentElement.style.colorScheme = raw
  }
} catch {
  /* localStorage 不可用时忽略，交给设置 store 处理 */
}

app.mount('#app')

/**
 * 禁用 WebView 的默认右键菜单。
 *
 * 这是桌面应用而不是网页：浏览器的「返回 / 刷新 / 另存为 / 打印 / 检查」在这里没有意义，
 * 而且会盖住我们自己的包管理右键菜单（PackageContextMenu）。
 * 输入框与可选中文本仍保留原生菜单，方便用户复制粘贴。
 */
document.addEventListener(
  'contextmenu',
  (event) => {
    const target = event.target as HTMLElement | null
    const editable = target?.closest('input, textarea, [contenteditable="true"]')
    if (editable) return
    event.preventDefault()
  },
  { capture: true },
)
