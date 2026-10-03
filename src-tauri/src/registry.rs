//! 镜像源配置：读取 / 解析 / 安全写回。
//!
//! 设计取舍：**优先直接读配置文件**，而不是执行 `npm config get`。
//! 原因：读文件更稳定（不受命令超时、输出格式变化影响），也能拿到「用户级 vs 默认」的区分。
//! 只有需要拿全局配置文件位置时才执行白名单命令。

use crate::error::{AppError, AppResult};
use crate::executor::{self, ExecRequest};
use crate::fsutil;
use crate::models::{RegistryConfig, RegistryEntry};
use crate::validate;
use crate::whitelist;
use std::path::{Path, PathBuf};

/// 已知镜像源提示（用于界面上打「官方 / 镜像」标签）
const KNOWN_REGISTRIES: &[(&str, &str)] = &[
    ("https://registry.npmjs.org", "npm 官方源"),
    ("https://registry.yarnpkg.com", "Yarn 官方源"),
    ("https://registry.npmmirror.com", "淘宝 npmmirror 镜像"),
    ("https://mirrors.huaweicloud.com/repository/npm/", "华为云镜像"),
    ("https://mirrors.cloud.tencent.com/npm/", "腾讯云镜像"),
    ("https://mirrors.tuna.tsinghua.edu.cn/pypi/web/simple", "清华 PyPI 镜像"),
    ("https://pypi.tuna.tsinghua.edu.cn/simple", "清华 PyPI 镜像"),
    ("https://mirrors.aliyun.com/pypi/simple/", "阿里云 PyPI 镜像"),
    ("https://pypi.org/simple", "PyPI 官方源"),
    ("https://pypi.mirrors.ustc.edu.cn/simple/", "中科大 PyPI 镜像"),
    ("https://mirrors.ustc.edu.cn/crates.io-index", "中科大 crates 镜像"),
    ("https://mirrors.tuna.tsinghua.edu.cn/crates.io-index", "清华 crates 镜像"),
    ("https://goproxy.cn", "七牛 Go 代理"),
    ("https://goproxy.io", "Go 代理"),
    ("https://mirrors.aliyun.com/goproxy/", "阿里云 Go 代理"),
];

fn hint_for(value: &str) -> Option<String> {
    let normalized = value.trim().trim_end_matches('/').to_ascii_lowercase();
    KNOWN_REGISTRIES
        .iter()
        .find(|(url, _)| {
            let u = url.trim_end_matches('/').to_ascii_lowercase();
            normalized == u || normalized.starts_with(&u)
        })
        .map(|(_, hint)| (*hint).to_string())
}

// ---------------------------------------------------------------------------
// 配置文件定位
// ---------------------------------------------------------------------------

/// 用户级配置文件的候选绝对路径（含环境变量注入的全局配置位置）
pub fn candidate_config_files(manager: &str, global_npmrc: Option<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let home = validate::home_dir();

    match manager {
        "npm" | "pnpm" | "bun" => {
            if let Some(h) = &home {
                out.push(h.join(".npmrc"));
            }
            if let Some(p) = global_npmrc {
                out.push(p);
            }
        }
        "yarn" => {
            if let Some(h) = &home {
                out.push(h.join(".yarnrc.yml"));
                out.push(h.join(".yarnrc"));
                out.push(h.join(".npmrc")); // yarn 1 会读 .npmrc
            }
        }
        "pip" | "uv" => {
            // 优先级参照 pip 官方文档
            if let Ok(explicit) = std::env::var("PIP_CONFIG_FILE") {
                if !explicit.trim().is_empty() {
                    out.push(PathBuf::from(explicit));
                }
            }
            if let Some(appdata) = validate::env_dir("APPDATA") {
                out.push(appdata.join("pip").join("pip.ini"));
            }
            if let Some(programdata) = validate::env_dir("PROGRAMDATA") {
                out.push(programdata.join("pip").join("pip.ini"));
            }
            if let Some(h) = &home {
                out.push(h.join(".pip").join("pip.conf"));
                out.push(h.join(".config").join("pip").join("pip.conf"));
            }
            #[cfg(unix)]
            {
                out.push(PathBuf::from("/etc/pip.conf"));
            }
        }
        "cargo" => {
            if let Some(h) = &home {
                out.push(h.join(".cargo").join("config.toml"));
                out.push(h.join(".cargo").join("config"));
            }
        }
        "go" => {
            if let Some(appdata) = validate::env_dir("APPDATA") {
                out.push(appdata.join("go").join("env"));
            }
            if let Some(h) = &home {
                out.push(h.join(".config").join("go").join("env"));
            }
        }
        "gem" => {
            if let Some(h) = &home {
                out.push(h.join(".gemrc"));
            }
        }
        _ => {}
    }
    out
}

/// 通过白名单命令取得 npm 全局 .npmrc 位置（失败则忽略）
fn npm_global_config_path(timeout_ms: u64) -> Option<PathBuf> {
    let req = ExecRequest::new("npm.cmd", &["config", "get", "globalconfig"])
        .with_timeout_ms(timeout_ms);
    let out = executor::run(&req).ok()?;
    let line = out.stdout.lines().next()?.trim().to_string();
    if line.is_empty() || line == "undefined" || line == "null" {
        return None;
    }
    let p = PathBuf::from(line);
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

/// 解析 `key=value` 风格配置文件（.npmrc / .yarnrc / gemrc / go env）
fn parse_kv(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, v)) = line.split_once('=').or_else(|| line.split_once(' ')) else {
            continue;
        };
        let key = k.trim().trim_start_matches('-').to_string();
        let value = v.trim().trim_matches('"').to_string();
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    out
}

/// 解析 INI 风格（pip.ini / pip.conf），只取 `[global]` 段
fn parse_ini_global(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_global = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_global = line[1..line.len() - 1].trim().eq_ignore_ascii_case("global");
            continue;
        }
        if !in_global {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

/// TOML 里的 `[registry]` / `[source.*]` 简单提取（够用即可，不引入 toml 依赖）
fn parse_toml_registry(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_string();
            if matches!(key.as_str(), "replace-with" | "registry" | "index" | "default") {
                out.push((key, v.trim().trim_matches('"').to_string()));
            }
        }
    }
    out
}

fn entries_from_pairs(manager: &str, pairs: Vec<(String, String)>) -> Vec<RegistryEntry> {
    // 每个管理器只关心与自己相关的键
    let interest: &[&str] = match manager {
        "npm" | "pnpm" | "yarn" | "bun" => &["registry", "strict-ssl", "proxy", "https-proxy"],
        "pip" | "uv" => &["index-url", "extra-index-url", "trusted-host", "timeout"],
        "cargo" => &["replace-with", "registry", "index", "default"],
        "go" => &["GOPROXY", "GOSUMDB", "GONOSUMDB", "GOPRIVATE"],
        "gem" => &["sources", ":sources"],
        _ => &[],
    };
    pairs
        .into_iter()
        .filter(|(k, _)| {
            interest.contains(&k.as_str())
                // 作用域源，如 @antfu:registry
                || (k.starts_with('@') && k.ends_with(":registry"))
        })
        .map(|(key, value)| RegistryEntry {
            hint: hint_for(&value),
            user_defined: true,
            key,
            value,
        })
        .collect()
}

/// 内置默认值（配置文件里没写时展示，帮助用户理解当前生效的源）
fn defaults_for(manager: &str) -> Vec<RegistryEntry> {
    match manager {
        "npm" | "bun" => vec![RegistryEntry {
            key: "registry".into(),
            value: "https://registry.npmjs.org/".into(),
            user_defined: false,
            hint: Some("npm 官方源（内置默认）".into()),
        }],
        "pnpm" => vec![RegistryEntry {
            key: "registry".into(),
            value: "https://registry.npmjs.org/".into(),
            user_defined: false,
            hint: Some("npm 官方源（内置默认）".into()),
        }],
        "yarn" => vec![RegistryEntry {
            key: "registry".into(),
            value: "https://registry.yarnpkg.com".into(),
            user_defined: false,
            hint: Some("Yarn 官方源（内置默认）".into()),
        }],
        "pip" | "uv" => vec![RegistryEntry {
            key: "index-url".into(),
            value: "https://pypi.org/simple".into(),
            user_defined: false,
            hint: Some("PyPI 官方源（内置默认）".into()),
        }],
        "cargo" => vec![RegistryEntry {
            key: "registry".into(),
            value: "https://crates.io/".into(),
            user_defined: false,
            hint: Some("crates.io 官方源（内置默认）".into()),
        }],
        "go" => vec![RegistryEntry {
            key: "GOPROXY".into(),
            value: "https://proxy.golang.org,direct".into(),
            user_defined: false,
            hint: Some("Go 官方代理（内置默认）".into()),
        }],
        _ => Vec::new(),
    }
}

/// 读取某管理器的镜像源现状。`manager` 必须已通过白名单校验。
pub fn read(manager: &str, timeout_ms: u64) -> AppResult<RegistryConfig> {
    if whitelist::find(manager).is_none() {
        return Err(AppError::invalid(format!("未知包管理器: {manager}")));
    }

    let global_npmrc = if manager == "npm" || manager == "pnpm" {
        npm_global_config_path(timeout_ms)
    } else {
        None
    };

    let candidates = candidate_config_files(manager, global_npmrc);
    let mut chosen: Option<PathBuf> = None;
    let mut pairs: Vec<(String, String)> = Vec::new();

    for path in &candidates {
        if !path.is_file() {
            continue;
        }
        let text = fsutil::read_text(path, 512 * 1024)?;
        let parsed = match manager {
            "npm" | "pnpm" | "yarn" | "bun" => parse_kv(&text),
            "pip" | "uv" => parse_ini_global(&text),
            "cargo" => parse_toml_registry(&text),
            "go" => parse_kv(&text),
            "gem" => parse_kv(&text),
            _ => Vec::new(),
        };
        if !parsed.is_empty() && chosen.is_none() {
            chosen = Some(path.clone());
            pairs = parsed;
            // 找到第一个有效来源即视为生效配置（优先级顺序已排好）
            break;
        }
    }

    let mut entries = entries_from_pairs(manager, pairs);
    if entries.is_empty() {
        entries = defaults_for(manager);
    }

    let raw = match &chosen {
        Some(p) => fsutil::read_text(p, 512 * 1024).unwrap_or_default(),
        None => String::new(),
    };

    let writable = match &chosen {
        Some(p) => p.is_file() && !p.metadata().map(|m| m.permissions().readonly()).unwrap_or(true),
        // 配置文件不存在时也允许创建（用户级路径）
        None => candidates.first().map(|_| true).unwrap_or(false),
    };

    Ok(RegistryConfig {
        manager_id: manager.to_string(),
        entries,
        raw,
        writable,
    })
}

/// 写回镜像源配置。
///
/// 安全措施：
/// 1. key 必须在白名单内，value 若为 URL 必须通过 `validate::registry_url`
/// 2. 先备份原文件为 `*.bak-<时间戳>`
/// 3. 只改动目标键所在行，其余内容逐字保留
/// 4. 使用「写临时文件 + 原子替换」，避免写坏配置
pub fn write(manager: &str, key: &str, value: &str) -> AppResult<String> {
    let def = whitelist::find(manager).ok_or_else(|| AppError::invalid(format!("未知包管理器: {manager}")))?;
    if !validate::config_key_allowed(manager, key) {
        return Err(AppError::forbidden(format!("不允许修改配置项: {manager}/{key}")));
    }
    let is_url = key == "registry" || key.ends_with(":registry") || key.contains("url") || key.contains("index");
    let value = if is_url && value.contains("://") {
        validate::registry_url(value)?.to_string()
    } else {
        // 非 URL 值同样禁止换行注入
        if value.contains(['\n', '\r']) {
            return Err(AppError::forbidden("配置值不能包含换行"));
        }
        value.trim().to_string()
    };

    let global_npmrc = if manager == "npm" || manager == "pnpm" {
        npm_global_config_path(20_000)
    } else {
        None
    };
    let candidates = candidate_config_files(manager, global_npmrc);
    let existing = candidates.iter().find(|p| p.is_file()).cloned();
    let target = match existing {
        Some(p) => p,
        None => candidates
            .into_iter()
            .next()
            .ok_or_else(|| AppError::io(format!("找不到 {} 的可写配置路径", def.name)))?,
    };

    let original = if target.is_file() { fsutil::read_text(&target, 512 * 1024)? } else { String::new() };

    // 备份
    let backup_path = if target.is_file() {
        let bak = target.with_extension(format!(
            "{}bak-{}",
            target.extension().and_then(|e| e.to_str()).map(|e| format!("{e}.")).unwrap_or_default(),
            fsutil::backup_suffix()
        ));
        fsutil::write_text(&bak, &original)?;
        Some(bak)
    } else {
        None
    };

    let updated = upsert_line(manager, &original, key, &value);

    // 原子替换：先写 .tmp 再 rename
    let tmp = target.with_extension("packinspect.tmp");
    fsutil::write_text(&tmp, &updated)?;
    std::fs::rename(&tmp, &target)
        .map_err(|e| AppError::io(format!("替换配置文件失败: {e}")))?;

    Ok(match backup_path {
        Some(b) => format!("已更新 {}，原文件备份为 {}", target.display(), b.display()),
        None => format!("已创建 {}", target.display()),
    })
}

/// 预览修改后的配置文本：不写盘，仅供前端 diff 展示。
/// `original` 为空时自动从当前实际配置文件读取，保证预览与真实结果一致。
pub fn preview(manager: &str, original: &str, key: &str, value: &str) -> String {
    let base = if original.trim().is_empty() {
        let global_npmrc =
            if manager == "npm" || manager == "pnpm" { npm_global_config_path(15_000) } else { None };
        candidate_config_files(manager, global_npmrc)
            .into_iter()
            .find(|p| p.is_file())
            .and_then(|p| fsutil::read_text(&p, 512 * 1024).ok())
            .unwrap_or_default()
    } else {
        original.to_string()
    };
    upsert_line(manager, &base, key, value)
}

/// 在配置文本中新增或替换某个键，保持其它行原样
fn upsert_line(manager: &str, original: &str, key: &str, value: &str) -> String {
    let sep = if matches!(manager, "pip" | "uv") { " = " } else { "=" };
    let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    let mut replaced = false;

    for line in lines.iter_mut() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with(';') || trimmed.is_empty() {
            continue;
        }
        let Some((k, _)) = trimmed.split_once('=') else { continue };
        if k.trim().eq_ignore_ascii_case(key) {
            *line = format!("{key}{sep}{value}");
            replaced = true;
            break;
        }
    }

    if !replaced {
        let needs_global_section = matches!(manager, "pip" | "uv") && !original.contains("[global]");
        if needs_global_section {
            if !lines.is_empty() && lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push("[global]".to_string());
        }
        lines.push(format!("{key}{sep}{value}"));
    }

    let mut out = lines.join("\n");
    if original.ends_with('\n') || !original.is_empty() {
        out.push('\n');
    }
    out
}

/// 列出可写配置项，供前端渲染表单
pub fn editable_keys(manager: &str) -> Vec<&'static str> {
    match manager {
        "npm" | "pnpm" | "yarn" | "bun" => vec!["registry", "strict-ssl", "proxy", "https-proxy"],
        "pip" | "uv" => vec!["index-url", "extra-index-url", "trusted-host", "timeout"],
        "cargo" => vec!["registry", "index"],
        "go" => vec!["GOPROXY", "GOSUMDB"],
        "gem" => vec!["sources"],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_existing_registry() {
        let src = "registry=https://old.example.com\nstrict-ssl=true\n";
        let out = upsert_line("npm", src, "registry", "https://registry.npmmirror.com");
        assert!(out.contains("registry=https://registry.npmmirror.com"));
        assert!(out.contains("strict-ssl=true"));
        assert_eq!(out.matches("registry=").count(), 1);
    }

    #[test]
    fn upsert_appends_when_missing() {
        let out = upsert_line("npm", "", "registry", "https://a.example.com");
        assert!(out.contains("registry=https://a.example.com"));
    }

    #[test]
    fn pip_upsert_creates_global_section() {
        let out = upsert_line("pip", "# comment\n", "index-url", "https://mirrors.aliyun.com/pypi/simple/");
        assert!(out.contains("[global]"));
        assert!(out.contains("index-url = https://mirrors.aliyun.com/pypi/simple/"));
    }

    #[test]
    fn parse_ini_only_reads_global() {
        let text = "[global]\nindex-url = https://x\n[install]\nfoo=bar\n";
        let pairs = parse_ini_global(text);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "index-url");
    }

    #[test]
    fn write_rejects_non_whitelisted_key() {
        assert!(write("npm", "evil-key", "1").is_err());
    }
}
