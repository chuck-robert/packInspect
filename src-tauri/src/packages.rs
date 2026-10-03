//! 已安装包枚举。
//!
//! 策略：**优先读磁盘，命令只用来「问路」**。
//! 例如 npm 用 `npm root -g` 问出全局目录，然后直接扫描该目录下的 package.json。
//! 这样即使某个命令超时/输出格式变化，仍能给出结果，且速度远快于解析命令文本。

use crate::error::AppResult;
use crate::executor::{self, ExecRequest};
use crate::fsutil;
use crate::models::PackageRecord;
use crate::whitelist;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 读取某个包目录的 package.json 摘要
fn node_package_at(dir: &Path, manager: &str, scope: &str, measure: bool) -> Option<PackageRecord> {
    let pkg_json = dir.join("package.json");
    if !pkg_json.is_file() {
        return None;
    }
    let value = fsutil::read_json(&pkg_json, 256 * 1024).ok().flatten()?;
    let name = value.get("name").and_then(Value::as_str).map(str::to_string);
    let version = value.get("version").and_then(Value::as_str).map(str::to_string);
    let description = value.get("description").and_then(Value::as_str).map(|s| s.chars().take(160).collect());
    Some(PackageRecord {
        // 极少数包缺 name 字段，用目录名兜底
        name: name.unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().to_string()),
        version,
        manager: manager.to_string(),
        scope: scope.to_string(),
        path: Some(dir.to_string_lossy().to_string()),
        size: if measure { Some(fsutil::dir_size(dir)) } else { None },
        redundant: false,
        redundant_reason: None,
        description,
        latest_version: None,
    })
}

/// 扫描一个 node_modules 风格目录（支持 `@scope/name` 两级结构）
pub fn scan_node_modules(root: &Path, manager: &str, scope: &str, measure: bool) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".bin" || name == ".package-lock.json" {
            continue;
        }
        if name.starts_with('@') {
            // 作用域目录：继续下探一层
            if let Ok(sub) = std::fs::read_dir(&path) {
                for sub_entry in sub.filter_map(Result::ok) {
                    let sub_path = sub_entry.path();
                    if sub_path.is_dir() {
                        if let Some(rec) = node_package_at(&sub_path, manager, scope, measure) {
                            out.push(rec);
                        }
                    }
                }
            }
            continue;
        }
        if let Some(rec) = node_package_at(&path, manager, scope, measure) {
            out.push(rec);
        }
    }
    out
}

/// 执行一条白名单命令并返回 stdout（失败时返回 None，由调用方兜底到磁盘扫描）
fn try_cmd(manager: &str, op: &str, timeout_ms: u64) -> Option<String> {
    let args = whitelist::op_args(manager, op)?;
    let def = whitelist::find(manager)?;
    let exe = executor::resolve_executable(def.exe_candidates)?;
    let req = ExecRequest::new(exe.to_string_lossy().to_string(), args).with_timeout_ms(timeout_ms);
    let out = executor::run_resolved(&exe, &req).ok()?;
    if out.success {
        Some(out.stdout)
    } else {
        None
    }
}

/// 取命令输出的第一行（用于 `npm root -g` 这类单值命令）
fn first_line(s: &str) -> Option<String> {
    s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.to_string())
}

// ---------------------------------------------------------------------------
// node 生态
// ---------------------------------------------------------------------------

/// npm / pnpm / yarn / bun 的全局包
pub fn node_global(manager: &str, timeout_ms: u64, measure: bool) -> (Option<PathBuf>, Vec<PackageRecord>) {
    let root = try_cmd(manager, "rootGlobal", timeout_ms).and_then(|s| first_line(&s)).map(PathBuf::from);
    let records = match &root {
        Some(r) if r.is_dir() => scan_node_modules(r, manager, "global", measure),
        _ => Vec::new(),
    };
    (root, records)
}

// ---------------------------------------------------------------------------
// python 生态
// ---------------------------------------------------------------------------

/// 由 pip 可执行文件位置推断 site-packages 目录（用于给条目附上安装路径）
fn python_site_packages(pip_exe: &Path) -> Option<PathBuf> {
    let scripts_dir = pip_exe.parent()?;
    let exe_name = pip_exe.file_stem()?.to_string_lossy().to_ascii_lowercase();
    // pip3.11.exe → 3.11 ；无版本号则用 python.exe / python3.exe 探一次
    let suffix = exe_name.trim_start_matches("pip").trim_start_matches('.');
    let versioned = if suffix.is_empty() {
        None
    } else {
        Some(scripts_dir.join(format!("python{suffix}.exe")))
    };
    let plain = [scripts_dir.join("python.exe"), scripts_dir.join("python3.exe")];

    let python = versioned
        .filter(|p| p.is_file())
        .or_else(|| plain.into_iter().find(|p| p.is_file()))?;

    let req = ExecRequest::new(python.to_string_lossy().to_string(), &[
        "-c",
        "import sysconfig;print(sysconfig.get_paths()['purelib'])",
    ])
    .with_timeout_ms(8_000);
    let out = executor::run_resolved(&python, &req).ok()?;
    if !out.success {
        return None;
    }
    let p = PathBuf::from(first_line(&out.stdout)?);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// pip / uv 的已安装包（含解释器基础环境，scope = system）
pub fn python_packages(manager: &str, timeout_ms: u64, measure: bool) -> (Option<PathBuf>, Vec<PackageRecord>) {
    let def = match whitelist::find(manager) {
        Some(d) => d,
        None => return (None, Vec::new()),
    };
    let Some(exe) = executor::resolve_executable(def.exe_candidates) else {
        return (None, Vec::new());
    };
    let args = match whitelist::op_args(manager, "listGlobal") {
        Some(a) => a,
        None => return (None, Vec::new()),
    };
    let req = ExecRequest::new(exe.to_string_lossy().to_string(), args).with_timeout_ms(timeout_ms);
    let site = python_site_packages(&exe);
    let Ok(out) = executor::run_resolved(&exe, &req) else {
        return (site, Vec::new());
    };
    if !out.success {
        return (site, Vec::new());
    }
    let Ok(value) = fsutil::parse_json_output(&out.stdout) else {
        return (site, Vec::new());
    };
    let list = value.as_array().cloned().unwrap_or_default();
    let mut records = Vec::with_capacity(list.len());
    for item in list {
        let name = item.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let version = item.get("version").and_then(Value::as_str).map(str::to_string);
        let path = site.as_ref().map(|sp| dist_info_path(sp, &name, version.as_deref()));
        records.push(PackageRecord {
            name,
            version,
            manager: manager.to_string(),
            scope: "system".to_string(),
            path: path.map(|p| p.to_string_lossy().to_string()).or_else(|| site.as_ref().map(|p| p.to_string_lossy().to_string())),
            size: None,
            redundant: false,
            redundant_reason: None,
            description: None,
            latest_version: None,
        });
    }
    if measure {
        if let Some(sp) = &site {
            for rec in records.iter_mut() {
                if let Some(p) = &rec.path {
                    let pb = PathBuf::from(p);
                    let target = if pb.is_dir() { pb } else { sp.clone() };
                    rec.size = Some(fsutil::dir_size(&target));
                }
            }
        }
    }
    (site, records)
}

/// 拼接 `xxx-1.0.0.dist-info` 目录名（PyPI 规范化：`-` 与 `_` 互换，小写比较）
fn dist_info_path(site: &Path, name: &str, version: Option<&str>) -> PathBuf {
    let normalized = name.to_ascii_lowercase().replace('-', "_");
    if let Ok(entries) = std::fs::read_dir(site) {
        for entry in entries.filter_map(Result::ok) {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let lower = dir_name.to_ascii_lowercase();
            let matches_name = lower.starts_with(&format!("{normalized}-")) || lower.starts_with(&format!("{}-", name.to_ascii_lowercase()));
            let matches_version = version.map(|v| lower.contains(&v.to_ascii_lowercase())).unwrap_or(true);
            if lower.ends_with(".dist-info") && matches_name && matches_version {
                return entry.path();
            }
        }
    }
    site.to_path_buf()
}

// ---------------------------------------------------------------------------
// rust / go
// ---------------------------------------------------------------------------

/// 解析 `cargo install --list` 输出
fn parse_cargo_install_list(text: &str) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        if !line.starts_with(' ') && line.contains(" v") && line.ends_with(':') {
            let head = line.trim_end_matches(':');
            if let Some((name, version)) = head.split_once(" v") {
                // 下一行缩进内容是真实安装路径
                let path = lines
                    .get(i + 1)
                    .map(|l| l.trim())
                    .filter(|l| l.starts_with('(') || Path::new(l).exists())
                    .map(|l| l.trim_matches(['(', ')']).to_string());
                out.push(PackageRecord {
                    name: name.trim().to_string(),
                    version: Some(version.trim().to_string()),
                    manager: "cargo".into(),
                    scope: "global".into(),
                    path,
                    size: None,
                    redundant: false,
                    redundant_reason: None,
                    description: None,
                    latest_version: None,
                });
            }
        }
        i += 1;
    }
    out
}

pub fn cargo_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    match try_cmd("cargo", "listGlobal", timeout_ms) {
        Some(text) => parse_cargo_install_list(&text),
        None => Vec::new(),
    }
}

/// 解析 GOMODCACHE 下的 `module@version` 目录结构
fn parse_gomodcache(root: &Path, measure: bool) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    // 结构：<cache>/<domain>/<path...>/<module>@<version>
    let walker = walkdir::WalkDir::new(root).min_depth(1).max_depth(6).follow_links(false);
    for entry in walker.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some((module, version)) = name.rsplit_once('@') else { continue };
        if version.is_empty() || module.is_empty() {
            continue;
        }
        let dir = entry.path();
        out.push(PackageRecord {
            name: module.to_string(),
            version: Some(version.to_string()),
            manager: "go".into(),
            scope: "global".into(),
            path: Some(dir.to_string_lossy().to_string()),
            size: if measure { Some(fsutil::dir_size(dir)) } else { None },
            redundant: false,
            redundant_reason: None,
            description: None,
            latest_version: None,
        });
        // 已到 module@version 层，不再向内枚举其源码目录
        // （WalkDir 无法在此剪枝，靠 depth 限制 + 名称过滤兜住）
    }
    out
}

pub fn go_packages(cache_root: Option<&Path>, measure: bool) -> Vec<PackageRecord> {
    match cache_root {
        Some(root) if root.is_dir() => parse_gomodcache(root, measure),
        _ => Vec::new(),
    }
}

/// 解析 `gem list --local --no-versions`
pub fn gem_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let Some(text) = try_cmd("gem", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|name| PackageRecord {
            name: name.to_string(),
            version: None,
            manager: "gem".into(),
            scope: "global".into(),
            path: None,
            size: None,
            redundant: false,
            redundant_reason: None,
            description: None,
            latest_version: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 冗余 / 旧版本识别
// ---------------------------------------------------------------------------

/// 标记同一包存在多个版本时，除最高版本外的其余版本为「旧版本可清理候选」。
///
/// 注意：这只做**标记**，真正的删除动作由 cleaner 按白名单执行，且默认排除项目依赖。
pub fn mark_old_versions(records: &mut [PackageRecord]) {
    use std::collections::HashMap;
    let mut by_key: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (idx, rec) in records.iter().enumerate() {
        if rec.scope == "local" {
            continue;
        }
        by_key.entry((rec.manager.clone(), rec.name.clone())).or_default().push(idx);
    }
    for ((manager, name), idxs) in by_key {
        if idxs.len() < 2 {
            continue;
        }
        // 同一 manager 下才有可比性，这是分组键保证的
        let _ = &manager;
        let best = idxs
            .iter()
            .filter_map(|i| records[*i].version.clone().map(|v| (v, *i)))
            .max_by(|a, b| compare_versions(&a.0, &b.0));
        if let Some((best_version, best_idx)) = best {
            for i in idxs {
                if i == best_idx {
                    continue;
                }
                records[i].redundant = true;
                records[i].redundant_reason =
                    Some(format!("{manager} 的 {name} 存在多版本，最新为 {best_version}"));
            }
        }
    }
}

/// 宽松的版本号比较（按数字段比较，非数字段按字典序），够用即可
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| -> Vec<String> {
        s.split(['.', '-', '+'])
            .map(|p| p.trim_start_matches('v').to_string())
            .collect()
    };
    let (va, vb) = (split(a), split(b));
    for i in 0..va.len().max(vb.len()) {
        let pa = va.get(i).map(String::as_str).unwrap_or("0");
        let pb = vb.get(i).map(String::as_str).unwrap_or("0");
        let na = pa.parse::<u64>();
        let nb = pb.parse::<u64>();
        let ord = match (na, nb) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => pa.cmp(pb),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

/// 供 manager 探测使用的便捷包装
pub fn command_first_line(manager: &str, op: &str, timeout_ms: u64) -> Option<String> {
    try_cmd(manager, op, timeout_ms).and_then(|s| first_line(&s))
}

/// 校验包管理器是否为已知项（避免把任意字符串当命令名）
pub fn is_known_manager(id: &str) -> bool {
    whitelist::find(id).is_some()
}

/// 统一入口：返回 `Result` 以兼容 Tauri command 签名
pub fn ensure_known(id: &str) -> AppResult<()> {
    if is_known_manager(id) {
        Ok(())
    } else {
        Err(crate::error::AppError::invalid(format!("不支持的包管理器: {id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PackageRecord;

    fn rec(name: &str, version: &str) -> PackageRecord {
        PackageRecord {
            name: name.into(),
            version: Some(version.into()),
            manager: "npm".into(),
            scope: "global".into(),
            path: None,
            size: None,
            redundant: false,
            redundant_reason: None,
            description: None,
            latest_version: None,
        }
    }

    #[test]
    fn version_compare_handles_numeric_segments() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10.0", "1.9.0"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("0.9", "1.0"), Less);
    }

    #[test]
    fn marks_only_older_duplicates() {
        let mut list = vec![rec("vue", "3.4.0"), rec("vue", "3.5.0"), rec("react", "18.0.0")];
        mark_old_versions(&mut list);
        assert!(list[0].redundant);
        assert!(!list[1].redundant);
        assert!(!list[2].redundant);
    }

    #[test]
    fn parses_cargo_install_list() {
        let text = "    cargo-edit v0.12.0:\n        cargo-edit.exe\n    ripgrep v14.1.0:\n        rg.exe\n";
        let parsed = parse_cargo_install_list(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "cargo-edit");
        assert_eq!(parsed[0].version.as_deref(), Some("0.12.0"));
    }
}
