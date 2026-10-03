//! 包图标生成。
//!
//! 设计取舍：**不联网抓取图标**。
//! - 抓 favicon 需要给 WebView 放行任意 https 域名，与「前端不接触外部资源」的安全约束冲突；
//! - 上千个包逐个联网会拖慢扫描、且离线环境必然空白。
//!
//! 因此这里生成内联 SVG（data URI）：按包名哈希取一组协调的深色系配色 + 首字母缩写。
//! 既有辨识度，又完全离线、体积小（每个约 300 字节），还能随主题换配色。

use std::collections::HashMap;
use std::sync::Mutex;

/// 每个管理器的品牌色，用于图标底色（深色主题下保持足够对比度）
fn brand_color(manager: &str) -> &'static str {
    match manager {
        "npm" => "#cb3837",
        "pnpm" => "#f9ad00",
        "yarn" => "#2188b6",
        "pip" => "#3776ab",
        "cargo" => "#dea584",
        "dotnet" => "#512bd4",
        "winget" => "#0078d4",
        "powershellget" => "#5391fe",
        "composer" => "#885630",
        "gem" => "#cc342d",
        "go" => "#00add8",
        "maven" => "#c71a36",
        "chocolatey" => "#80b5e3",
        "scoop" => "#7c4dff",
        "conda" => "#43b02a",
        "dart" => "#0175c2",
        "luarocks" => "#000080",
        "cpan" => "#3f5f8f",
        _ => "#4c9aff",
    }
}

/// 由字符串算一个稳定的色相偏移，避免同色图标糊成一片
fn hue_shift(seed: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in seed.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    // 限制在 ±12 度内，保持品牌色基调
    hash % 25
}

/// 提取用于图标的首字母缩写。
/// - `@scope/name` → `name`
/// - `phpunit/phpunit` → 取最后一段
/// - `github.com/spf13/cobra` → 取最后一段
fn initials(name: &str) -> String {
    let tail = name.rsplit('/').next().unwrap_or(name);
    let cleaned: String = tail
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    let letters: Vec<char> = cleaned.chars().filter(|c| c.is_alphanumeric()).collect();
    if letters.is_empty() {
        return "?".to_string();
    }
    // 连字符/点分名（如 eslint-plugin-vue）取各段首字母，否则取前两个字母
    if cleaned.contains(['-', '.', '_']) {
        let parts: Vec<&str> = cleaned.split(['-', '.', '_']).filter(|p| !p.is_empty()).collect();
        let take: String = parts.iter().take(2).filter_map(|p| p.chars().next()).collect();
        if take.len() >= 2 {
            return take.to_uppercase();
        }
    }
    letters.iter().take(2).collect::<String>().to_uppercase()
}

/// 把颜色按色相偏移调亮/调暗一点
fn shift_hex(hex: &str, shift: u32) -> String {
    let parse = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
    if hex.len() != 7 {
        return hex.to_string();
    }
    let (r, g, b) = (parse(&hex[1..3]), parse(&hex[3..5]), parse(&hex[5..7]));
    let amount = shift as f64 / 100.0 * 40.0;
    let clamp = |v: u32| -> u32 { v.min(255) };
    let nr = clamp((r as f64 + amount).round() as u32);
    let ng = clamp((g as f64 + amount).round() as u32);
    let nb = clamp((b as f64 + amount).round() as u32);
    format!("#{nr:02x}{ng:02x}{nb:02x}")
}

/// 生成内联 SVG 图标（data URI）。
///
/// 这里用字符串拼接而不是 `format!`：SVG 里含大量 `{}` 属性与 CSS 花括号，
/// 若走 format 需要全部转义，拼接反而更清晰、也不易出错。
pub fn monogram_for(manager: &str, package: &str) -> String {
    let base = brand_color(manager);
    let top = shift_hex(base, hue_shift(package));
    let label = initials(package);
    let font_size = if label.len() > 1 { 13.0 } else { 17.0 };

    // 注意用 r##"..."## ：SVG 文本以 `"` 开头，r#"..."# 会被提前终止
    let mut svg = String::with_capacity(560);
    svg.push_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color=""##,
    );
    svg.push_str(&top);
    svg.push_str(r##""/><stop offset="1" stop-color=""##);
    svg.push_str(base);
    svg.push_str(r##""/></linearGradient></defs><rect width="32" height="32" rx="7" fill="url(#g)"/><text x="16" y="16" fill="#ffffff" font-family="Segoe UI,sans-serif" font-size=""##);
    svg.push_str(&font_size.to_string());
    svg.push_str(r##"" font-weight="600" text-anchor="middle" dominant-baseline="central">"##);
    svg.push_str(&label);
    svg.push_str("</text></svg>");

    format!("data:image/svg+xml;base64,{}", base64_encode(svg.as_bytes()))
}

/// 生成管理器的图标（用品牌色 + 名称缩写），供侧边栏使用
pub fn manager_icon(manager: &str, display_name: &str) -> String {
    monogram_for(manager, display_name)
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

/// 图标缓存：同一 (manager, package) 只生成一次。
/// 扫描上千个包时，重复生成 SVG 是纯浪费。
#[derive(Default)]
pub struct IconCache {
    inner: Mutex<HashMap<String, String>>,
}

impl IconCache {
    pub fn get_or_create(&self, manager: &str, package: &str) -> String {
        let key = format!("{manager}\u{1}{package}");
        if let Ok(guard) = self.inner.lock() {
            if let Some(hit) = guard.get(&key) {
                return hit.clone();
            }
        }
        let icon = monogram_for(manager, package);
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(key, icon.clone());
        }
        icon
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

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn initials_handle_scopes_and_separators() {
        assert_eq!(initials("@anthropic-ai/claude-code"), "CC");
        assert_eq!(initials("eslint-plugin-vue"), "EP");
        assert_eq!(initials("phpunit/phpunit"), "PH");
        assert_eq!(initials("vue"), "VU");
        assert_eq!(initials("github.com/spf13/cobra"), "CO");
        assert_eq!(initials("..."), "?");
    }

    #[test]
    fn generated_icon_is_a_decodable_svg_data_uri() {
        let icon = monogram_for("npm", "vue");
        assert!(icon.starts_with("data:image/svg+xml;base64,"));
        let payload = icon.trim_start_matches("data:image/svg+xml;base64,");
        assert!(!payload.is_empty());
        // "<svg" 的 base64 前缀固定为 PHN2Zy
        assert!(payload.starts_with("PHN2Zy"), "SVG 开头应为 <svg，实际: {payload:.16}");
    }

    #[test]
    fn icon_is_stable_for_same_input() {
        assert_eq!(monogram_for("pip", "requests"), monogram_for("pip", "requests"));
        // 不同包名应产生不同图标（色相偏移或字母至少有一个不同）
        assert_ne!(monogram_for("pip", "requests"), monogram_for("pip", "flask"));
    }

    #[test]
    fn cache_reuses_generated_value() {
        let cache = IconCache::default();
        let a = cache.get_or_create("npm", "vue");
        let b = cache.get_or_create("npm", "vue");
        assert_eq!(a, b);
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }
}
