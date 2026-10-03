//! 包管理器品牌 logo 生成。
//!
//! 设计目标：**离线可用的矢量 logo**，不引入任何外部资源。
//!
//! 为什么自绘而不是抓官方 PNG/favicon：
//! - 抓图需要给 WebView 放行任意 https 域名，与「前端不接触外部资源」的安全约束冲突；
//! - 离线环境必然空白；
//! - 官方 logo 多为位图，在高 DPI 下会糊。
//!
//! 做法：把每个包管理器的**识别性视觉特征**（形状 + 官方品牌色）用少量 SVG 路径重绘。
//! 因为要同时适配深色与浅色主题，每个 logo 提供两个配色变体，前端按当前主题选取。
//!
//! 新增管理器时只需在 `manager_logo_svg` 里加一个 case；未覆盖的会退回
//! 「品牌色底 + 名称缩写」，保证永远不会空白。

use std::collections::HashMap;
use std::sync::Mutex;

/// 主题变体
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn parse(value: &str) -> Theme {
        match value.to_ascii_lowercase().as_str() {
            "light" => Theme::Light,
            _ => Theme::Dark,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
}

/// 每个管理器的品牌色（同时用于通用回退样式）
pub fn brand_color(manager: &str) -> &'static str {
    match manager {
        "npm" => "#cb3837",
        "pnpm" => "#f9ad00",
        "yarn" => "#2188b6",
        "pip" => "#3775a9",
        "cargo" => "#a8642a",
        "dotnet" => "#68217a",
        "winget" => "#0078d4",
        "powershellget" => "#2f6fdb",
        "composer" => "#8b6036",
        "gem" => "#c81e2c",
        "go" => "#00a6d6",
        "maven" => "#b02a30",
        "chocolatey" => "#8a6240",
        "scoop" => "#6b4fbb",
        "conda" => "#2f9e44",
        "dart" => "#0175c2",
        "luarocks" => "#2b3f8f",
        "cpan" => "#3f5f8f",
        // ---- 四期：语言生态与运行时 ----
        "bun" => "#f472b6",
        "deno" => "#70ffaf",
        "julia" => "#9558b2",
        "mix" => "#4b275f",
        // ---- 四期：构建工具与系统级 ----
        "gradle" => "#02303a",
        "brew" => "#fbb040",
        "vcpkg" => "#4a3aff",
        "conan" => "#0b8bd4",
        "swift" => "#f05138",
        "cocoapods" => "#ee3322",
        // ---- 五期：Linux 发行版 ----
        "apt" => "#a80030",
        "pacman" => "#1793d1",
        "dnf" => "#3c6eb4",
        "flatpak" => "#4a90d9",
        "snap" => "#82bea0",
        "pipx" => "#3775a9",
        // ---- 六期：语言工具链 ----
        "opam" => "#e0812c",
        "dub" => "#b03931",
        "nimble" => "#ffe953",
        "cabal" => "#5e5086",
        "stack" => "#5d4f85",
        _ => "#4c9aff",
    }
}

/// 提取用于通用回退样式的缩写
fn initials(name: &str) -> String {
    let letters: Vec<char> = name.chars().filter(|c| c.is_alphanumeric()).collect();
    letters.iter().take(2).collect::<String>().to_uppercase()
}

/// 四期之后新增的管理器的矢量标记。
///
/// 【为什么不画官方 logo】
/// 官方 logo 多为复杂路径且有商标限制（见 README 说明）。这里用**简洁几何形 +
/// 品牌色**表达"这是哪个工具"，可辨认即可 —— 每个形状都刻意做得彼此不同，
/// 扫一眼能区分，比一堆字母缩写方块强得多。
///
/// 返回 `None` 时调用方会退回「品牌色 + 首字母」的通用样式。
fn generic_mark(manager: &str, color: &str, ink: &str) -> Option<String> {
    let body = match manager {
        // bun：粉色底 + 白色小圆点（保留"点"的意象）
        "bun" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<circle cx="16" cy="19" r="6" fill="{ink}"/>"##,
                r##"<circle cx="11" cy="10" r="2" fill="{ink}"/>"##,
                r##"<circle cx="16" cy="8" r="2" fill="{ink}"/>"##,
                r##"<circle cx="21" cy="10" r="2" fill="{ink}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // deno：深色底 + 绿色同心弧（恐龙轮廓太复杂，用其"光环"意象）
        "deno" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#1c1c1c"/>"##,
                r##"<circle cx="16" cy="16" r="9" fill="none" stroke="{color}" stroke-width="2.5"/>"##,
                r##"<circle cx="16" cy="16" r="3.5" fill="{color}"/>"##
            ),
            color = color
        ),
        // Julia：紫/绿/红三色圆点（其 logo 就是三个点的组合）。
        // 颜色写死为品牌三色，因此不需要 format 参数。
        "julia" => concat!(
            r##"<rect width="32" height="32" rx="6" fill="#1b1b2b"/>"##,
            r##"<circle cx="11" cy="12" r="3.4" fill="#9558b2"/>"##,
            r##"<circle cx="21" cy="12" r="3.4" fill="#389826"/>"##,
            r##"<circle cx="16" cy="21" r="3.4" fill="#cb3c33"/>"##
        )
        .to_string(),
        // mix：紫色六边形（Elixir 的化学意象）
        "mix" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M16 6l8.5 5v10L16 26l-8.5-5V11z" fill="none" stroke="{ink}" stroke-width="2"/>"##,
                r##"<circle cx="16" cy="16" r="2.6" fill="{ink}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // gradle：深青底 + 向上的台阶（构建）
        "gradle" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M8 23h5V15h5V11h6" fill="none" stroke="{ink}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>"##
            ),
            color = color,
            ink = ink
        ),
        // brew：琥珀色底 + 啤酒杯
        "brew" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M9 10h11v13a2 2 0 01-2 2h-7a2 2 0 01-2-2z" fill="{ink}"/>"##,
                r##"<path d="M20 13h3a3 3 0 010 6h-3" fill="none" stroke="{ink}" stroke-width="2.4"/>"##
            ),
            color = color,
            ink = ink
        ),
        // vcpkg：紫色底 + 层叠的方块（依赖栈）
        "vcpkg" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<rect x="9" y="18" width="14" height="6" rx="1.5" fill="{ink}"/>"##,
                r##"<rect x="9" y="11" width="14" height="6" rx="1.5" fill="{ink}" opacity="0.75"/>"##,
                r##"<rect x="9" y="4" width="14" height="6" rx="1.5" fill="{ink}" opacity="0.5"/>"##
            ),
            color = color,
            ink = ink
        ),
        // conan：蓝色底 + 由中心发散的节点（包依赖图）
        "conan" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<circle cx="16" cy="16" r="3.2" fill="{ink}"/>"##,
                r##"<circle cx="8" cy="9" r="2" fill="{ink}"/>"##,
                r##"<circle cx="24" cy="9" r="2" fill="{ink}"/>"##,
                r##"<circle cx="8" cy="24" r="2" fill="{ink}"/>"##,
                r##"<circle cx="24" cy="24" r="2" fill="{ink}"/>"##,
                r##"<path d="M14 14l-4.5-3.5M18 14l4.5-3.5M14 18l-4.5 3.5M18 18l4.5 3.5" stroke="{ink}" stroke-width="1.6"/>"##
            ),
            color = color,
            ink = ink
        ),
        // swift：橙红底 + 飞鸟剪影（折线意象）
        "swift" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M6 20c6 1 12-3 15-8-5 2-9 1-11-1 3 0 6-1 8-3-3 .5-7 .5-10-1 4-1 8-4 9-6" fill="none" stroke="{ink}" stroke-width="2.2" stroke-linecap="round"/>"##
            ),
            color = color,
            ink = ink
        ),
        // cocoapods：红色底 + 菱形（Pod 意象）
        "cocoapods" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M16 5l9 11-9 11-9-11z" fill="{ink}"/>"##,
                r##"<path d="M16 11l4 5-4 5-4-5z" fill="{color}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // apt：暗红底 + 三条横线（软件包堆叠）
        "apt" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<rect x="8" y="9" width="16" height="4" rx="2" fill="{ink}"/>"##,
                r##"<rect x="8" y="15" width="16" height="4" rx="2" fill="{ink}" opacity="0.7"/>"##,
                r##"<rect x="8" y="21" width="16" height="4" rx="2" fill="{ink}" opacity="0.45"/>"##
            ),
            color = color,
            ink = ink
        ),
        // pacman：蓝色底 + 吃豆人缺口圆
        "pacman" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M16 6a10 10 0 100 20 10 10 0 00-7.1-2.9L16 16l-7.1-7.1A10 10 0 0016 6z" fill="{ink}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // dnf：蓝色底 + 向下的箭头（下载/安装）
        "dnf" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M16 6v13" stroke="{ink}" stroke-width="3" stroke-linecap="round"/>"##,
                r##"<path d="M9 15l7 7 7-7" fill="none" stroke="{ink}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>"##,
                r##"<rect x="8" y="25" width="16" height="3" rx="1.5" fill="{ink}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // flatpak：蓝色底 + 立方体（沙盒容器）
        "flatpak" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M16 5l9 5v12l-9 5-9-5V10z" fill="none" stroke="{ink}" stroke-width="2.2" stroke-linejoin="round"/>"##,
                r##"<path d="M7 10l9 5 9-5M16 15v12" stroke="{ink}" stroke-width="1.6" opacity="0.8"/>"##
            ),
            color = color,
            ink = ink
        ),
        // snap：淡绿底 + 右上角的斜角方块（其 logo 的方格意象）
        "snap" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#252525"/>"##,
                r##"<path d="M16 5l10 5.5v11L16 27 6 21.5v-11z" fill="{color}"/>"##,
                r##"<path d="M16 5l10 5.5-10 5.5-10-5.5z" fill="#ffffff" opacity="0.35"/>"##
            ),
            color = color
        ),
        // pipx：蓝底 + 方框内的小方块（隔离环境）
        "pipx" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<rect x="7" y="7" width="18" height="18" rx="3" fill="none" stroke="{ink}" stroke-width="2.2"/>"##,
                r##"<rect x="13" y="13" width="6" height="6" rx="1.5" fill="{ink}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // opam：橙色底 + 骆驼（OCaml 吉祥物太复杂，用"包裹"意象）
        "opam" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M6 12l10-5 10 5v10l-10 5-10-5z" fill="none" stroke="{ink}" stroke-width="2.2" stroke-linejoin="round"/>"##,
                r##"<path d="M6 12l10 5 10-5M16 17v10" stroke="{ink}" stroke-width="1.8"/>"##
            ),
            color = color,
            ink = ink
        ),
        // dub：红底 + 齿轮（D 语言构建工具）
        "dub" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<circle cx="16" cy="16" r="5" fill="none" stroke="{ink}" stroke-width="2.4"/>"##,
                r##"<path d="M16 5v4M16 23v4M5 16h4M23 16h4M8.2 8.2l2.8 2.8M21 21l2.8 2.8M23.8 8.2L21 11M11 21l-2.8 2.8" stroke="{ink}" stroke-width="2.4" stroke-linecap="round"/>"##
            ),
            color = color,
            ink = ink
        ),
        // nimble：黄底 + 皇冠（Nim 的皇冠意象，用深色描边保证可见）
        "nimble" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M7 22l-1-11 6 5 4-8 4 8 6-5-1 11z" fill="#1f2328"/>"##
            ),
            color = color
        ),
        // cabal：紫底 + 函数式符号 λ
        "cabal" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M20 8h-3l-6 16H8M13 14h9" fill="none" stroke="{ink}" stroke-width="2.6" stroke-linecap="round"/>"##
            ),
            color = color,
            ink = ink
        ),
        // stack：紫底 + 三层堆叠（名字即"栈"）
        "stack" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<rect x="8" y="20" width="16" height="5" rx="1.5" fill="{ink}"/>"##,
                r##"<rect x="11" y="13" width="10" height="5" rx="1.5" fill="{ink}" opacity="0.75"/>"##,
                r##"<rect x="14" y="6" width="4" height="5" rx="1.5" fill="{ink}" opacity="0.55"/>"##
            ),
            color = color,
            ink = ink
        ),
        _ => return None,
    };
    Some(body)
}

/// 生成某个包管理器的 logo（内联 SVG data URI）
pub fn manager_logo_svg(manager: &str, name: &str, theme: Theme) -> String {
    // 浅色主题下把品牌色压深一点，保证在浅底上仍有对比度
    let color = match theme {
        Theme::Dark => brand_color(manager).to_string(),
        Theme::Light => darken(brand_color(manager), 0.12),
    };
    // 在深色底上作画用亮色描边；在浅色底上作画用深色描边
    let ink = match theme {
        Theme::Dark => "#ffffff",
        Theme::Light => "#1f2328",
    };
    let muted = match theme {
        Theme::Dark => "#c9d1d9",
        Theme::Light => "#57606a",
    };

    let body = match manager {
        // ---- npm：红底白色方块标 ----
        "npm" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M7 9h18v14h-5v-9h-3v9h-4v-9H9v9H7z" fill="{ink}"/>"##
            ),
            color = color,
            ink = ink
        ),
        // ---- pnpm：深底 + 分块色块 ----
        "pnpm" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#1b1b1b"/>"##,
                r##"<rect x="6" y="7" width="9" height="9" rx="1.5" fill="{color}"/>"##,
                r##"<rect x="17" y="7" width="9" height="9" rx="1.5" fill="{color}"/>"##,
                r##"<rect x="6" y="18" width="9" height="9" rx="1.5" fill="{color}"/>"##,
                r##"<rect x="17" y="18" width="9" height="9" rx="1.5" fill="#ffffff" opacity="0.35"/>"##
            ),
            color = color
        ),
        // ---- yarn：线轴抽象 ----
        "yarn" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<circle cx="16" cy="16" r="8.5" fill="none" stroke="{ink}" stroke-width="2.6"/>"##,
                r##"<circle cx="16" cy="16" r="2.4" fill="{ink}"/>"##,
                r##"<path d="M16 7.5v3M16 21.5v3M7.5 16h3M21.5 16h3" stroke="{ink}" stroke-width="2.2" stroke-linecap="round"/>"##
            ),
            color = color,
            ink = ink
        ),
        // ---- pip：Python 双色带 ----
        "pip" => concat!(
            r##"<rect width="32" height="32" rx="6" fill="#2b3a4a"/>"##,
            r##"<path d="M8 7h16v7a4 4 0 0 1-4 4h-8z" fill="#3775a9"/>"##,
            r##"<path d="M24 25H8v-7a4 4 0 0 1 4-4h8z" fill="#ffd43b"/>"##,
            r##"<circle cx="12.5" cy="11" r="1.4" fill="#ffffff"/>"##,
            r##"<circle cx="19.5" cy="21" r="1.4" fill="#1f2328"/>"##
        )
        .to_string(),
        // ---- cargo：Rust 齿轮 + 螺旋 ----
        "cargo" => concat!(
            r##"<rect width="32" height="32" rx="6" fill="#2b2118"/>"##,
            r##"<circle cx="16" cy="16" r="9" fill="#a8642a"/>"##,
            r##"<path d="M16 9.5a6.5 6.5 0 1 0 6.5 6.5" fill="none" stroke="#1b120b" stroke-width="2.4" stroke-linecap="round"/>"##,
            r##"<path d="M16 13.5a2.5 2.5 0 1 0 2.5 2.5" fill="none" stroke="#1b120b" stroke-width="2.2" stroke-linecap="round"/>"##
        )
        .to_string(),
        // ---- .NET：紫底 + 斜线 ----
        "dotnet" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                r##"<path d="M11 10v12" stroke="{ink}" stroke-width="2.6" stroke-linecap="round"/>"##,
                r##"<path d="M13.5 10l6 12" stroke="#00b294" stroke-width="2.6" stroke-linecap="round"/>"##,
                r##"<path d="M21.5 12.5h4.5M21.5 19.5h4.5" stroke="{ink}" stroke-width="2.2" stroke-linecap="round"/>"##
            ),
            color = color,
            ink = ink
        ),
        // ---- winget：Windows 四格 ----
        "winget" => concat!(
            r##"<rect width="32" height="32" rx="6" fill="#0b2b45"/>"##,
            r##"<rect x="7" y="7" width="8" height="8" fill="#0078d4"/>"##,
            r##"<rect x="17" y="7" width="8" height="8" fill="#2b88d8"/>"##,
            r##"<rect x="7" y="17" width="8" height="8" fill="#2b88d8"/>"##,
            r##"<rect x="17" y="17" width="8" height="8" fill="#0078d4"/>"##
        )
        .to_string(),
        // ---- PowerShell：终端提示符 ----
        "powershellget" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#0b2545"/>"##,
                r##"<path d="M8 11l6 5-6 5" fill="none" stroke="#5391fe" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"/>"##,
                r##"<path d="M17 21h7" stroke="{muted}" stroke-width="2.4" stroke-linecap="round"/>"##
            ),
            muted = muted
        ),
        // ---- composer：圆形递进 ----
        "composer" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#2f2418"/>"##,
                r##"<circle cx="16" cy="16" r="9" fill="none" stroke="{color}" stroke-width="3"/>"##,
                r##"<path d="M20 12.5a5.5 5.5 0 0 0-8 4.5" fill="none" stroke="{muted}" stroke-width="2.4" stroke-linecap="round"/>"##
            ),
            color = color,
            muted = muted
        ),
        // ---- gem：宝石切面 ----
        "gem" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#2a1215"/>"##,
                r##"<path d="M10 8h12l5 6-11 11L5 14z" fill="{color}"/>"##,
                r##"<path d="M10 8l6 21 6-21M5 14h22" fill="none" stroke="#ffffff" stroke-opacity="0.45" stroke-width="1.3"/>"##
            ),
            color = color
        ),
        // ---- go：gopher 风格双色圆 ----
        "go" => concat!(
            r##"<rect width="32" height="32" rx="6" fill="#0b2b33"/>"##,
            r##"<circle cx="16" cy="16" r="9" fill="#00a6d6"/>"##,
            r##"<circle cx="12.5" cy="13.5" r="2.1" fill="#ffffff"/>"##,
            r##"<circle cx="19.5" cy="13.5" r="2.1" fill="#ffffff"/>"##,
            r##"<path d="M11 20c2.6 1.8 7.4 1.8 10 0" fill="none" stroke="#0b2b33" stroke-width="2" stroke-linecap="round"/>"##
        )
        .to_string(),
        // ---- maven：M 形尖顶 ----
        "maven" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#2a1416"/>"##,
                r##"<path d="M6 24V10l10 8 10-8v14" fill="none" stroke="{color}" stroke-width="3" stroke-linejoin="round" stroke-linecap="round"/>"##
            ),
            color = color
        ),
        // ---- Chocolatey：巧克力方块 ----
        "chocolatey" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#2b1d12"/>"##,
                r##"<rect x="7" y="7" width="18" height="18" rx="3" fill="{color}"/>"##,
                r##"<path d="M7 16h18M16 7v18" stroke="#2b1d12" stroke-width="1.6" opacity="0.5"/>"##
            ),
            color = color
        ),
        // ---- Scoop：铲形 ----
        "scoop" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#1e1633"/>"##,
                r##"<path d="M8 9h16l-2.5 9H10.5z" fill="{color}"/>"##,
                r##"<path d="M16 18v6" stroke="{muted}" stroke-width="2.4" stroke-linecap="round"/>"##
            ),
            color = color,
            muted = muted
        ),
        // ---- conda：双环 ----
        "conda" => concat!(
            r##"<rect width="32" height="32" rx="6" fill="#0d2413"/>"##,
            r##"<circle cx="12" cy="16" r="5.2" fill="none" stroke="#43b02a" stroke-width="2.6"/>"##,
            r##"<circle cx="20" cy="16" r="5.2" fill="none" stroke="#2f9e44" stroke-width="2.6"/>"##
        )
        .to_string(),
        // ---- dart：菱形 ----
        "dart" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#0b2438"/>"##,
                r##"<path d="M16 5l11 11-11 11L5 16z" fill="{color}"/>"##,
                r##"<path d="M16 5v22M5 16h22" stroke="#ffffff" stroke-opacity="0.25" stroke-width="1.4"/>"##
            ),
            color = color
        ),
        // ---- luarocks：月牙 + 岩块 ----
        "luarocks" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#0e1230"/>"##,
                r##"<path d="M9 22a8 8 0 0 1 14-6 7 7 0 0 0-9-2 7 7 0 0 0-2 9z" fill="{color}"/>"##,
                r##"<circle cx="21" cy="21" r="3.2" fill="{muted}" opacity="0.55"/>"##
            ),
            color = color,
            muted = muted
        ),
        // ---- cpan：骆驼剪影 ----
        "cpan" => format!(
            concat!(
                r##"<rect width="32" height="32" rx="6" fill="#141d2e"/>"##,
                r##"<path d="M10 24v-4c0-2 1-3 2-4l-1-4 3 2 3-1 2-3 1 3 3 1-1 3 1 3v4z" fill="{color}"/>"##
            ),
            color = color
        ),
        // ---- 通用回退：品牌色方块 + 名称缩写 ----
        _ => {
            let label = initials(name);
            if let Some(shape) = generic_mark(manager, &color, ink) {
                shape
            } else {
                format!(
                    concat!(
                        r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                        r##"<text x="16" y="16" fill="{ink}" font-family="Segoe UI,sans-serif" font-size="13" font-weight="700" text-anchor="middle" dominant-baseline="central">{label}</text>"##
                    ),
                    color = color,
                    ink = ink,
                    label = label
                )
            }
        }
    };

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">{body}</svg>"##
    );
    format!("data:image/svg+xml;base64,{}", base64_encode(svg.as_bytes()))
}

/// 把十六进制颜色按比例压深（用于浅色主题下的对比度）
fn darken(hex: &str, amount: f64) -> String {
    if hex.len() != 7 {
        return hex.to_string();
    }
    let parse = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
    let (r, g, b) = (parse(&hex[1..3]), parse(&hex[3..5]), parse(&hex[5..7]));
    let scale = |v: u32| -> u32 { ((v as f64) * (1.0 - amount)).round().max(0.0) as u32 };
    format!("#{:02x}{:02x}{:02x}", scale(r), scale(g), scale(b))
}

// ---------------------------------------------------------------------------
// base64（自己实现，避免为一个小功能引入依赖）
// ---------------------------------------------------------------------------

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 缓存
// ---------------------------------------------------------------------------

/// logo 缓存：按 `manager|theme` 缓存 —— 同一管理器的 logo 只生成一次
#[derive(Default)]
pub struct LogoCache {
    inner: Mutex<HashMap<String, String>>,
}

impl LogoCache {
    pub fn get_or_create(&self, manager: &str, name: &str, theme: Theme) -> String {
        let key = format!("{manager}|{}", theme.as_str());
        if let Ok(guard) = self.inner.lock() {
            if let Some(hit) = guard.get(&key) {
                return hit.clone();
            }
        }
        let logo = manager_logo_svg(manager, name, theme);
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(key, logo.clone());
        }
        logo
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitelist;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn every_manager_gets_a_logo_in_both_themes() {
        for def in whitelist::MANAGERS {
            for theme in [Theme::Dark, Theme::Light] {
                let logo = manager_logo_svg(def.id, def.name, theme);
                let payload = logo.trim_start_matches("data:image/svg+xml;base64,");
                assert!(
                    logo.starts_with("data:image/svg+xml;base64,"),
                    "{} 的 logo 前缀不对",
                    def.id
                );
                // "<svg" 的 base64 前缀固定为 PHN2Zy
                assert!(payload.starts_with("PHN2Zy"), "{} 的 logo 不是 SVG", def.id);
                assert!(payload.len() > 120, "{} 的 logo 内容过短", def.id);
            }
        }
    }

    #[test]
    fn theme_variants_differ() {
        let dark = manager_logo_svg("dotnet", "dotnet", Theme::Dark);
        let light = manager_logo_svg("dotnet", "dotnet", Theme::Light);
        assert_ne!(dark, light, "深浅主题应产生不同配色");
    }

    /// 四期之后新增的管理器必须有**各自不同**的矢量标记。
    ///
    /// 这条测试的作用：防止有人新增管理器后忘了加标记，让它们全都退化成
    /// 通用首字母方块 —— 侧边栏一列方块会完全失去辨识度。
    #[test]
    fn new_managers_have_distinct_marks() {
        let new_ids = [
            "bun", "deno", "julia", "mix", "gradle", "brew", "vcpkg", "conan", "swift",
            "cocoapods", "apt", "pacman", "dnf", "flatpak", "snap", "pipx", "opam", "dub",
            "nimble", "cabal", "stack",
        ];
        let mut seen: Vec<(String, String)> = Vec::new();
        for id in new_ids {
            let mark = generic_mark(id, "#4c9aff", "#ffffff");
            assert!(mark.is_some(), "{id} 缺少专用矢量标记，会退化成首字母方块");
            let body = mark.unwrap();
            assert!(body.len() > 60, "{id} 的标记内容过短");
            // 每个标记都应引用传入的色值或自定义色，但形状必须彼此不同
            for (other_id, other_body) in &seen {
                assert_ne!(
                    &body, other_body,
                    "{id} 与 {other_id} 的标记完全相同，失去辨识度"
                );
            }
            seen.push((id.to_string(), body));
        }
        // 未知 id 必须返回 None，以便调用方退回通用样式
        assert!(generic_mark("nonexistent-manager", "#fff", "#000").is_none());
    }

    #[test]
    fn unknown_manager_falls_back_to_monogram() {
        let logo = manager_logo_svg("totally-unknown", "Zoo Keeper", Theme::Dark);
        let payload = logo.trim_start_matches("data:image/svg+xml;base64,");
        assert!(payload.starts_with("PHN2Zy"), "回退也应产出 SVG");
        // 回退样式含 <text>：base64("<text") == "PHRleHQ"
        assert!(payload.contains("PHRleHQ"), "回退样式应包含文字缩写");
    }

    #[test]
    fn darken_reduces_brightness() {
        assert_eq!(darken("#ffffff", 0.5), "#808080");
        assert_eq!(darken("#000000", 0.5), "#000000");
        assert_eq!(darken("bogus", 0.5), "bogus");
    }

    #[test]
    fn cache_is_keyed_by_manager_and_theme() {
        let cache = LogoCache::default();
        let a = cache.get_or_create("npm", "npm", Theme::Dark);
        let b = cache.get_or_create("npm", "npm", Theme::Dark);
        let c = cache.get_or_create("npm", "npm", Theme::Light);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn theme_parsing_is_lenient() {
        assert_eq!(Theme::parse("light"), Theme::Light);
        assert_eq!(Theme::parse("LIGHT"), Theme::Light);
        assert_eq!(Theme::parse("dark"), Theme::Dark);
        assert_eq!(Theme::parse(""), Theme::Dark);
        assert_eq!(Theme::parse("nonsense"), Theme::Dark);
    }
}
