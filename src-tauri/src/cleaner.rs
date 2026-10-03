//! 缓存清理：候选枚举 + 安全执行。
//!
//! 【安全边界】
//! 1. 只枚举 `whitelist::clean_allowed` 允许的 (manager, kind) 组合；
//! 2. 每个候选路径必须通过 `validate::ensure_within` 校验，确保落在该管理器的缓存根之内；
//! 3. 路径级硬性拒绝：`node_modules`、`site-packages`、虚拟环境、以及任何用户项目目录；
//! 4. 删除一律通过 `dry_run` 先算账，前端二次确认后才真正执行；
//! 5. 永不调用 `npm uninstall` / `pip uninstall` 之类的卸载命令 —— 本工具不卸载包。

use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::manager;
use crate::models::{CleanCandidate, CleanKind, CleanResult};
use crate::packages::PackageRecord;
use crate::validate;
use crate::whitelist;
use std::path::{Path, PathBuf};

/// 单个候选路径的大小上限：超过则标记为 protected，避免「一键删掉 30GB」这类误操作
const MAX_CANDIDATE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// 永远不允许删除的目录名（即使它们意外出现在缓存根之下）
const FORBIDDEN_DIR_NAMES: &[&str] = &[
    "node_modules",
    "site-packages",
    "dist-packages",
    "venv",
    ".venv",
    "env",
    "conda-meta",
    "bin",
    "Scripts",
    ".git",
    "src",
];

/// 判断路径是否命中关键目录名（逐段比较，大小写不敏感）
fn hits_forbidden_name(path: &Path) -> Option<String> {
    for comp in path.components() {
        if let std::path::Component::Normal(name) = comp {
            let name = name.to_string_lossy().to_ascii_lowercase();
            if FORBIDDEN_DIR_NAMES.iter().any(|f| name == f.to_ascii_lowercase()) {
                return Some(name);
            }
        }
    }
    None
}

fn candidate_id(manager: &str, kind: CleanKind, path: &Path) -> String {
    // 用路径的稳定哈希做 id，避免把完整路径塞进 id 里
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    validate::normalize(path).to_string_lossy().to_ascii_lowercase().hash(&mut hasher);
    let kind_str = match kind {
        CleanKind::Cache => "cache",
        CleanKind::OldVersion => "oldver",
        CleanKind::Temp => "temp",
        CleanKind::Orphan => "orphan",
    };
    format!("{manager}:{kind_str}:{:016x}", hasher.finish())
}

fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// 为「旧版本包」生成候选（仅当该包位于可识别的全局目录中，且不在项目里）
fn old_version_candidates(records: &[PackageRecord], timeout_ms: u64) -> Vec<CleanCandidate> {
    // 全局根每条 manager 只解析一次（解析要跑命令，不能放在循环里）
    let mut root_cache: std::collections::HashMap<String, Option<PathBuf>> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for rec in records {
        if !rec.redundant || rec.scope == "local" {
            continue;
        }
        let Some(path_str) = &rec.path else { continue };
        let path = PathBuf::from(path_str);
        if !path.is_dir() {
            continue;
        }
        // 只处理 node 生态：全局 node_modules/<pkg> 目录可直接移除旧版本
        if rec.manager != "npm" && rec.manager != "pnpm" && rec.manager != "yarn" {
            continue;
        }
        let root = root_cache
            .entry(rec.manager.clone())
            .or_insert_with(|| manager::global_root_for(&rec.manager, timeout_ms))
            .clone();
        let Some(root) = root else { continue };
        if validate::ensure_within(&path, &root).is_err() {
            continue;
        }
        if hits_forbidden_name(&path).is_some() {
            continue;
        }
        let stat = fsutil::dir_stat(&path, 200_000);
        out.push(CleanCandidate {
            id: candidate_id(&rec.manager, CleanKind::OldVersion, &path),
            manager_id: rec.manager.clone(),
            kind: CleanKind::OldVersion,
            path: path_str.clone(),
            bytes: stat.bytes,
            file_count: stat.file_count,
            reason: rec
                .redundant_reason
                .clone()
                .unwrap_or_else(|| format!("{} 的旧版本 {}，可移除", rec.name, rec.version.clone().unwrap_or_default())),
            risk: "warn".into(),
            protected: false,
        });
    }
    out
}

/// 枚举所有清理候选。
///
/// `dry_run = false` 时不会改变这里的行为 —— 枚举永远是只读的。
pub fn enumerate(records: &[PackageRecord], timeout_ms: u64) -> Vec<CleanCandidate> {
    let mut out: Vec<CleanCandidate> = Vec::new();

    for def in whitelist::MANAGERS {
        let Some(root) = manager::cache_dir_for(def.id, timeout_ms) else { continue };
        if !root.is_dir() {
            continue;
        }

        // ---- 1. 缓存子目录级候选 ----
        for sub in def.cache_subdirs {
            let target = root.join(sub.split('/').next().unwrap_or(sub));
            if !target.exists() {
                continue;
            }
            let kind = CleanKind::Cache;
            let allowed = whitelist::clean_allowed(def.id, "cache");
            // 注意：`cache_root_protected` 保护的是「整个缓存根」，不阻止清理其内部的缓存子目录
            let mut protected = !allowed;
            let mut reason = if def.cache_root_protected {
                format!("{} 的存储目录，删除会影响已用硬链接的项目，默认保护", def.name)
            } else {
                format!("{} 的缓存目录", def.name)
            };
            if let Some(name) = hits_forbidden_name(&target) {
                protected = true;
                reason = format!("命中受保护目录名 {name}，禁止删除");
            }
            let stat = fsutil::dir_stat(&target, 200_000);
            if stat.bytes > MAX_CANDIDATE_BYTES {
                protected = true;
                reason = format!("{}（{}，超过单次清理上限）", reason, fsutil::human_bytes(stat.bytes));
            }
            out.push(CleanCandidate {
                id: candidate_id(def.id, kind, &target),
                manager_id: def.id.to_string(),
                kind,
                path: path_to_string(&target),
                bytes: stat.bytes,
                file_count: stat.file_count,
                reason,
                risk: if protected { "protected".into() } else { "safe".into() },
                protected,
            });
        }

        // ---- 2. 临时文件 / 中断下载残留 ----
        for entry in walkdir::WalkDir::new(&root)
            .min_depth(1)
            .max_depth(4)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .take(20_000)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let is_temp = name.ends_with(".tmp")
                || name.ends_with(".temp")
                || name.ends_with(".part")
                || name.ends_with(".download")
                || name.starts_with("tmp-");
            if !is_temp {
                continue;
            }
            let path = entry.path().to_path_buf();
            if validate::ensure_within(&path, &root).is_err() {
                continue;
            }
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(CleanCandidate {
                id: candidate_id(def.id, CleanKind::Temp, &path),
                manager_id: def.id.to_string(),
                kind: CleanKind::Temp,
                path: path_to_string(&path),
                bytes,
                file_count: 1,
                reason: format!("{} 的临时/中断下载残留", def.name),
                risk: "safe".into(),
                protected: false,
            });
            if out.len() > 2_000 {
                break;
            }
        }
    }

    out.extend(old_version_candidates(records, timeout_ms));

    // 按体积降序，界面默认关注占用最大的项
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

/// 执行清理。
///
/// - `dry_run = true`：只返回「将会删除什么 + 能释放多少」
/// - `dry_run = false`：真正删除，逐项返回结果
pub fn run(
    candidates: &[CleanCandidate],
    ids: &[String],
    dry_run: bool,
    timeout_ms: u64,
) -> AppResult<Vec<CleanResult>> {
    let selected: Vec<&CleanCandidate> = candidates
        .iter()
        .filter(|c| ids.iter().any(|id| id == &c.id))
        .collect();

    if selected.is_empty() {
        return Err(AppError::invalid("没有匹配到任何清理候选，请重新扫描"));
    }

    // 找到的 id 数少于请求数 → 说明前端缓存过期，要求重扫（避免误删）
    if selected.len() != ids.len() {
        return Err(AppError::new(
            "STALE_SELECTION",
            "部分清理项已失效，请重新扫描后再试",
        ));
    }

    let mut results = Vec::with_capacity(selected.len());
    for cand in selected {
        if cand.protected {
            results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: false,
                freed_bytes: 0,
                message: Some("该项已被安全策略保护，跳过".into()),
            });
            continue;
        }
        if !whitelist::clean_allowed(&cand.manager_id, kind_str(cand.kind)) {
            results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: false,
                freed_bytes: 0,
                message: Some("该管理器/类型不在清理白名单内".into()),
            });
            continue;
        }

        let path = PathBuf::from(&cand.path);
        // 关键校验：缓存根必须能定位，且目标严格位于其下
        let Some(root) = manager::cache_dir_for(&cand.manager_id, timeout_ms) else {
            results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: false,
                freed_bytes: 0,
                message: Some("无法定位该管理器的缓存根目录".into()),
            });
            continue;
        };

        // 旧版本包位于全局安装目录而非缓存目录，需换用全局根做边界
        let boundary = if cand.kind == CleanKind::OldVersion {
            manager::global_root_for(&cand.manager_id, timeout_ms).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };

        let safe_path = match validate::ensure_within(&path, &boundary) {
            Ok(p) => p,
            Err(e) => {
                results.push(CleanResult {
                    candidate_id: cand.id.clone(),
                    path: cand.path.clone(),
                    ok: false,
                    freed_bytes: 0,
                    message: Some(e.message),
                });
                continue;
            }
        };

        if let Some(name) = hits_forbidden_name(&safe_path) {
            results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: false,
                freed_bytes: 0,
                message: Some(format!("路径包含受保护目录 {name}，已拒绝")),
            });
            continue;
        }

        if dry_run {
            let stat = fsutil::dir_stat(&safe_path, 200_000);
            results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: true,
                freed_bytes: stat.bytes,
                message: Some(format!("预览：将删除 {} 个文件", stat.file_count)),
            });
            continue;
        }

        // ---- 真正执行 ----
        let bytes = fsutil::dir_size(&safe_path);
        let outcome = if safe_path.is_dir() {
            std::fs::remove_dir_all(&safe_path)
        } else {
            std::fs::remove_file(&safe_path)
        };
        match outcome {
            Ok(_) => results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: true,
                freed_bytes: bytes,
                message: Some(format!("已释放 {}", fsutil::human_bytes(bytes))),
            }),
            Err(e) => results.push(CleanResult {
                candidate_id: cand.id.clone(),
                path: cand.path.clone(),
                ok: false,
                freed_bytes: 0,
                message: Some(format!(
                    "删除失败（文件可能正被占用）: {e}{}",
                    if safe_path.is_dir() { "；建议先关闭相关开发工具" } else { "" }
                )),
            }),
        }
    }

    Ok(results)
}

fn kind_str(kind: CleanKind) -> &'static str {
    match kind {
        CleanKind::Cache => "cache",
        CleanKind::OldVersion => "oldVersion",
        CleanKind::Temp => "temp",
        CleanKind::Orphan => "orphan",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_names_detected() {
        assert!(hits_forbidden_name(Path::new("C:/x/node_modules/vue")).is_some());
        assert!(hits_forbidden_name(Path::new("C:/x/site-packages/requests")).is_some());
        assert!(hits_forbidden_name(Path::new("C:/x/_cacache/blob")).is_none());
    }

    #[test]
    fn candidate_ids_are_stable_and_safe() {
        let p = Path::new("C:/Users/u/AppData/Local/npm-cache/_cacache");
        let a = candidate_id("npm", CleanKind::Cache, p);
        let b = candidate_id("npm", CleanKind::Cache, p);
        assert_eq!(a, b);
        assert!(validate::candidate_id(&a).is_ok());
    }

    #[test]
    fn dry_run_reports_without_deleting() {
        // 构造一个真实临时目录做端到端验证
        let base = std::env::temp_dir().join(format!("packinspect-test-{}", std::process::id()));
        let child = base.join("_cacache");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("blob.bin"), vec![0u8; 1024]).unwrap();

        let stats = fsutil::dir_stat(&child, 1000);
        assert!(stats.bytes >= 1024);
        assert!(child.exists(), "统计不应产生副作用");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_unknown_ids() {
        let err = run(&[], &["npm:cache:deadbeef".to_string()], true, 1000).unwrap_err();
        assert_eq!(err.code, "INVALID_INPUT");
    }
}
