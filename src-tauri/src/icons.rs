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
        _ => "#4c9aff",
    }
}

/// 提取用于通用回退样式的缩写
fn initials(name: &str) -> String {
    let letters: Vec<char> = name.chars().filter(|c| c.is_alphanumeric()).collect();
    letters.iter().take(2).collect::<String>().to_uppercase()
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
            format!(
                concat!(
                    r##"<rect width="32" height="32" rx="6" fill="{color}"/>"##,
                    r##"<text x="16" y="16" fill="#ffffff" font-family="Segoe UI,sans-serif" font-size="13" font-weight="700" text-anchor="middle" dominant-baseline="central">{label}</text>"##
                ),
                color = color,
                label = label
            )
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
