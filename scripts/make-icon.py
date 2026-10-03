"""生成 PackInspect 的应用图标。

设计意图（而不是"随便画个方块"）：
- **包裹箱**：这是"包管理"最直接的意象，`package` 一词本身就是包裹
- **放大镜**：工具的核心动作是「扫描 / 检查」，放大镜让"inspect"这个动作一眼可读
- 两者叠加时把箱子放在左下、放大镜压在右下，形成"正在检查这个箱子"的关系
- 深色底 + 亮青主色：与应用的深色主题一致；蓝紫渐变沿用原占位图的品牌感

输出 Tauri 需要的三个文件：
  icons/32x32.png
  icons/128x128.png
  icons/icon.ico   （多尺寸：16/24/32/48/64/128/256，Windows 各场景各取所需）
"""

from PIL import Image, ImageDraw

# ---- 配色 ----
BG_TOP = (30, 41, 59)        # 深蓝灰（与主题的 --bg-raised 接近）
BG_BOTTOM = (17, 24, 39)     # 更深
ACCENT = (76, 154, 255)      # 应用主色 --accent
ACCENT_DARK = (37, 99, 235)
BOX_LIGHT = (147, 197, 253)  # 箱子亮面
BOX_MID = (96, 165, 250)     # 箱子侧面
BOX_DARK = (59, 130, 246)    # 箱子暗面
WHITE = (255, 255, 255)

# 以 1024 为基准作图，最后再缩放 —— 这样小尺寸下的抗锯齿更均匀
BASE = 1024


def rounded_mask(size: int, radius_ratio: float = 0.22) -> Image.Image:
    """生成圆角矩形遮罩（用 4 倍超采样保证边缘平滑）"""
    ss = 4
    big = Image.new("L", (size * ss, size * ss), 0)
    ImageDraw.Draw(big).rounded_rectangle(
        (0, 0, size * ss - 1, size * ss - 1),
        radius=int(size * ss * radius_ratio),
        fill=255,
    )
    return big.resize((size, size), Image.LANCZOS)


def vertical_gradient(size: int, top, bottom) -> Image.Image:
    img = Image.new("RGB", (1, size))
    px = img.load()
    for y in range(size):
        t = y / max(1, size - 1)
        px[0, y] = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
    return img.resize((size, size), Image.BILINEAR)


def draw_box(d: ImageDraw.ImageDraw, x: int, y: int, w: int, h: int):
    """一个等距视角的包裹箱：顶面（菱形）+ 左右两个侧面。

    比例上刻意让 h ≈ 1.2w（近立方），否则会显得像一块砖。
    """
    depth = int(w * 0.30)          # 等距投影的水平偏移
    top_h = int(w * 0.17)          # 顶面高度由**宽度**决定，保证菱形比例恒定

    # 顶面：上-右-下-左 四个顶点
    top = [
        (x + depth // 2, y),
        (x + w, y + top_h),
        (x + w - depth // 2, y + top_h * 2),
        (x, y + top_h),
    ]
    d.polygon(top, fill=BOX_LIGHT)

    # 左侧面
    d.polygon(
        [(x, y + top_h), (x + w - depth // 2, y + top_h * 2),
         (x + w - depth // 2, y + h), (x, y + h - top_h)],
        fill=BOX_MID,
    )
    # 右侧面（更暗，制造体积感）
    d.polygon(
        [(x + w - depth // 2, y + top_h * 2), (x + w, y + top_h),
         (x + w, y + h - top_h), (x + w - depth // 2, y + h)],
        fill=BOX_DARK,
    )

    # 箱盖封条缝：只在顶面上，用比顶面略亮的同色系
    seam_w = max(2, w // 70)
    apex_x = x + w // 2 - depth // 4
    d.line([(apex_x, y), (apex_x, y + top_h * 2)],
           fill=(191, 219, 254), width=seam_w)


def draw_magnifier(d: ImageDraw.ImageDraw, cx: int, cy: int, r: int, ring: int):
    """右下角的放大镜：圆环 + 手柄。压在箱子上表示"正在检查它"。"""
    d.ellipse((cx - r, cy - r, cx + r, cy + r), outline=WHITE, width=ring)
    # 镜片内部用主色淡填充，避免"空心圈"显得空洞
    d.ellipse((cx - r + ring, cy - r + ring, cx + r - ring, cy + r - ring),
              fill=(15, 23, 42))
    # 手柄：45° 指向右下
    import math
    a = math.radians(45)
    x0 = cx + int((r - ring * 0.4) * math.cos(a))
    y0 = cy + int((r - ring * 0.4) * math.sin(a))
    x1 = cx + int((r + r * 0.62) * math.cos(a))
    y1 = cy + int((r + r * 0.62) * math.sin(a))
    d.line([(x0, y0), (x1, y1)], fill=WHITE, width=int(ring * 1.5))


def build_base() -> Image.Image:
    """画出 1024×1024 的基准图"""
    icon = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    bg = vertical_gradient(BASE, BG_TOP, BG_BOTTOM).convert("RGBA")
    icon.paste(bg, (0, 0), rounded_mask(BASE))

    d = ImageDraw.Draw(icon)
    # 箱子放左上：宽 0.44、高 0.42（近立方，不是砖块）
    draw_box(d, x=int(BASE * 0.13), y=int(BASE * 0.26),
             w=int(BASE * 0.44), h=int(BASE * 0.42))
    # 放大镜压右下角：只在角部与箱子轻微相交
    draw_magnifier(d, cx=int(BASE * 0.70), cy=int(BASE * 0.68),
                   r=int(BASE * 0.165), ring=max(6, int(BASE * 0.030)))
    return icon


def main():
    import os
    out_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "icons")
    out_dir = os.path.normpath(out_dir)
    os.makedirs(out_dir, exist_ok=True)

    base = build_base()

    # PNG：Tauri 需要 32 / 128；额外给 256 供未来使用
    for size in (32, 128, 256):
        img = base.resize((size, size), Image.LANCZOS)
        path = os.path.join(out_dir, f"{size}x{size}.png")
        img.save(path, "PNG", optimize=True)
        print(f"  写出 {path}")

    # ICO：Windows 会在不同场景取不同尺寸（任务栏 32、资源管理器 48/256、安装程序 16/32）
    sizes = [16, 24, 32, 48, 64, 128, 256]
    frames = [base.resize((s, s), Image.LANCZOS) for s in sizes]
    ico_path = os.path.join(out_dir, "icon.ico")
    frames[-1].save(
        ico_path, "ICO",
        sizes=[(s, s) for s in sizes],
        append_images=frames[:-1],
    )
    print(f"  写出 {ico_path}（{len(sizes)} 个尺寸: {sizes}）")
    print("完成。")


if __name__ == "__main__":
    main()
