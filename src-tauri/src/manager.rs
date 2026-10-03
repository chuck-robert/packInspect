//! 包管理器探测：定位可执行文件、版本、全局根目录、缓存目录、配置文件。

use crate::error::AppResult;
use crate::executor;
use crate::fsutil;
use crate::models::ManagerInfo;
use crate::packages;
use crate::registry;
use crate::validate;
use crate::whitelist;
use std::path::PathBuf;

/// 探测单个包管理器。任何子步骤失败都降级为 warning，不影响整体返回。
pub fn detect(id: &str, timeout_ms: u64, with_registry: bool) -> AppResult<ManagerInfo> {
    let def = whitelist::find(id).ok_or_else(|| crate::error::AppError::invalid(format!("不支持: {id}")))?;
    let mut warnings: Vec<String> = Vec::new();

    let exe = executor::resolve_executable(def.exe_candidates);
    if exe.is_none() {
        warnings.push(format!("未在 PATH 中找到 {}", def.exe_candidates.join(" / ")));
    }

    let version = match &exe {
        Some(_) => {
            let args = whitelist::op_args(id, "version").unwrap_or(&[]);
            let req = executor::ExecRequest::new(
                exe.as_ref().unwrap().to_string_lossy().to_string(),
                args,
            )
            .with_timeout_ms(timeout_ms.min(15_000));
            match executor::run_resolved(exe.as_ref().unwrap(), &req) {
                Ok(out) if out.success => out
                    .stdout
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(|l| l.chars().take(80).collect()),
                Ok(out) => {
                    warnings.push(format!("版本命令失败: {}", out.failure_hint()));
                    None
                }
                Err(e) => {
                    warnings.push(format!("版本命令异常: {}", e.message));
                    None
                }
            }
        }
        None => None,
    };

    let global_root = global_root_for(id, timeout_ms);
    let cache_dir = cache_dir_for(id, timeout_ms);
    let config_file = registry::candidate_config_files(id, None)
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().to_string());

    let registry_config = if with_registry && exe.is_some() {
        match registry::read(id, timeout_ms) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                warnings.push(format!("读取镜像源失败: {}", e.message));
                None
            }
        }
    } else {
        None
    };

    Ok(ManagerInfo {
        id: def.id.to_string(),
        name: def.name.to_string(),
        language: def.language.to_string(),
        detected: exe.is_some(),
        version,
        exe_path: exe.map(|p| p.to_string_lossy().to_string()),
        global_root: global_root.map(|p| p.to_string_lossy().to_string()),
        cache_dir: cache_dir.map(|p| p.to_string_lossy().to_string()),
        config_file,
        registry: registry_config,
        warnings,
    })
}

/// 全局包安装根目录
pub fn global_root_for(id: &str, timeout_ms: u64) -> Option<PathBuf> {
    // 1) 命令优先（最准确）
    if let Some(line) = packages::command_first_line(id, "rootGlobal", timeout_ms) {
        let p = PathBuf::from(line.trim());
        if p.is_dir() {
            return Some(p);
        }
    }
    // 2) 磁盘兜底
    let home = validate::home_dir();
    let local = validate::env_dir("LOCALAPPDATA");
    let appdata = validate::env_dir("APPDATA");
    match id {
        "npm" => {
            let prefix = packages::command_first_line("npm", "prefixGlobal", timeout_ms)
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
                .or_else(|| appdata.clone());
            prefix.map(|p| p.join("node_modules")).filter(|p| p.is_dir())
        }
        "pnpm" => {
            let pnpm_home = validate::env_dir("PNPM_HOME").or_else(|| local.map(|l| l.join("pnpm")));
            // pnpm v7+ 使用 <pnpm home>/global/<version>/node_modules，这里取最新的一个
            let global_dir = pnpm_home?.join("global");
            let mut versions: Vec<PathBuf> = std::fs::read_dir(&global_dir)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            versions.last().map(|v| v.join("node_modules")).filter(|p| p.is_dir())
        }
        "yarn" => {
            let base = local.or(appdata)?;
            let p = base.join("Yarn").join("Data").join("global").join("node_modules");
            if p.is_dir() {
                Some(p)
            } else {
                home.map(|h| h.join(".config").join("yarn").join("global").join("node_modules")).filter(|p| p.is_dir())
            }
        }
        "bun" => home.map(|h| h.join(".bun").join("install").join("global").join("node_modules")).filter(|p| p.is_dir()),
        "cargo" => home.map(|h| h.join(".cargo").join("bin")).filter(|p| p.is_dir()),
        "go" => packages::command_first_line("go", "cacheDir", timeout_ms).map(PathBuf::from).filter(|p| p.is_dir()),
        "gem" => packages::command_first_line("gem", "rootGlobal", timeout_ms).map(PathBuf::from).filter(|p| p.is_dir()),
        _ => None,
    }
}

/// 缓存目录
pub fn cache_dir_for(id: &str, timeout_ms: u64) -> Option<PathBuf> {
    // 先问命令（npm/pnpm/yarn/uv 都支持）
    if let Some(line) = packages::command_first_line(id, "cacheDir", timeout_ms) {
        let raw = line.trim().trim_matches('"');
        let p = validate::expand_tilde(raw);
        if p.is_dir() {
            return Some(p);
        }
    }

    let home = validate::home_dir()?;
    let local = validate::env_dir("LOCALAPPDATA");
    let appdata = validate::env_dir("APPDATA");

    /// 优先返回真实存在的候选；都不存在时返回第一个候选，供界面展示「预期路径」
    fn prefer(mut list: Vec<PathBuf>) -> Option<PathBuf> {
        if list.is_empty() {
            return None;
        }
        if let Some(found) = list.iter().find(|p| p.is_dir()) {
            return Some(found.clone());
        }
        Some(list.remove(0))
    }

    let candidate: Option<PathBuf> = match id {
        "npm" => prefer(
            [
                local.as_ref().map(|l| l.join("npm-cache")),      // npm v7+
                appdata.as_ref().map(|a| a.join("npm-cache")),    // npm v6 及更早
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        "pnpm" => prefer(
            [
                local.as_ref().map(|l| l.join("pnpm").join("store")),
                local.as_ref().map(|l| l.join("pnpm-store")),
                Some(home.join(".pnpm-store")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        "yarn" => prefer(vec![local
            .clone()
            .unwrap_or_else(|| home.clone())
            .join("Yarn")
            .join("Cache")]),
        "bun" => prefer(vec![home.join(".bun").join("install").join("cache")]),
        "pip" => prefer(vec![local
            .clone()
            .unwrap_or_else(|| home.join(".cache"))
            .join("pip")
            .join("Cache")]),
        "uv" => prefer(vec![local.unwrap_or_else(|| home.join(".cache")).join("uv")]),
        "cargo" => prefer(vec![home.join(".cargo").join("registry")]),
        "go" => prefer(vec![home.join("go").join("pkg").join("mod")]),
        "gem" => prefer(vec![home.join(".gem")]),
        _ => None,
    };

    candidate
}

/// 判断两个路径是否指向同一位置
pub fn same_dir(a: &PathBuf, b: &PathBuf) -> bool {
    validate::normalize(a) == validate::normalize(b)
}

/// 缓存统计（供 scan 与 cache 命令共用）
pub fn cache_stats(id: &str, timeout_ms: u64) -> AppResult<crate::models::CacheStats> {
    let path = cache_dir_for(id, timeout_ms)
        .ok_or_else(|| crate::error::AppError::not_installed(&format!("{id} 的缓存目录")))?;
    let stat = fsutil::dir_stat(&path, fsutil::MAX_SCAN_ENTRIES);
    let children = fsutil::children_stat(&path, 100_000)
        .into_iter()
        .take(60)
        .map(|(name, child_path, cs)| crate::models::CacheChild {
            name,
            path: child_path.to_string_lossy().to_string(),
            bytes: cs.bytes,
            file_count: cs.file_count,
        })
        .collect();

    Ok(crate::models::CacheStats {
        manager_id: id.to_string(),
        path: path.to_string_lossy().to_string(),
        exists: path.is_dir(),
        total_bytes: stat.bytes,
        file_count: stat.file_count,
        last_modified: stat.last_modified.map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.to_rfc3339()
        }),
        children,
        truncated: stat.truncated,
    })
}
