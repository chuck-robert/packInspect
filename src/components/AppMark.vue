<script setup lang="ts">
/**
 * PackInspect 自身的品牌标记（包裹箱 + 放大镜）。
 *
 * 【为什么要有这个组件】
 * 标题栏左下角原先是一个写着 "PI" 的渐变方块，与应用图标毫无关系 ——
 * 于是「应用图标换了，标题栏还是老样子」。这里改成与 `scripts/make-icon.py`
 * 生成的图标同构的矢量图形，一眼能对上。
 *
 * 为什么用内联 SVG 而不是引 `icons/*.png`：
 * - 22px 下矢量比位图缩放更锐利，不会发糊
 * - 不用为前端再复制一份图片资源，也不会因资源没打包而变成空洞
 * - 可以跟随主题变量取色
 *
 * 图形与 make-icon.py 的对应关系：
 *   背景圆角方块  ←→ 深色底（BG_TOP/BG_BOTTOM）
 *   浅蓝顶面      ←→ BOX_LIGHT，并保留那条封条缝
 *   中/深蓝侧面   ←→ BOX_MID / BOX_DARK（暗面营造体积感）
 *   白色圆环+手柄 ←→ 放大镜（简化为白色描边，小尺寸下最清晰）
 */
withDefaults(
  defineProps<{
    /** 显示边长（px）。默认与标题栏原有的 18px 一致 */
    size?: number
  }>(),
  { size: 18 },
)
</script>

<template>
  <svg
    class="app-mark"
    :width="size"
    :height="size"
    viewBox="0 0 1024 1024"
    role="img"
    aria-label="PackInspect"
  >
    <!-- 深色圆角底：与图标外框一致 -->
    <rect x="0" y="0" width="1024" height="1024" rx="230" fill="#111827" />

    <!-- 包裹箱：等距视角，右上为亮顶面 -->
    <!-- 顶面（菱形） -->
    <polygon points="268,300 659,300 719,487 168,466" fill="#93C5FD" />
    <!-- 左侧面 -->
    <polygon points="141,490 634,487 634,783 141,720" fill="#60A5FA" />
    <!-- 右侧面（更暗，形成体积感） -->
    <polygon points="634,487 779,437 779,688 634,783" fill="#3B82F6" />
    <!-- 封条缝：让"这是个箱子"更明确，小尺寸下也撑得住 -->
    <line x1="522" y1="300" x2="522" y2="491" stroke="#BFDBFE" stroke-width="26" />

    <!-- 放大镜：压在箱子右下角 -->
    <circle cx="717" cy="696" r="169" fill="#111827" stroke="#FFFFFF" stroke-width="62" />
    <line
      x1="836"
      y1="815"
      x2="921"
      y2="900"
      stroke="#FFFFFF"
      stroke-width="78"
      stroke-linecap="round"
    />
  </svg>
</template>

<style scoped>
.app-mark {
  display: block;
  flex: none;
}
</style>
