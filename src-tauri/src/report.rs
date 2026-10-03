//! 扫描编排 + 报告导出。

use crate::cleaner;
use crate::error::AppResult;
use crate::executor;
use crate::fsutil;
use crate::manager;
use crate::models::*;
use crate::packages;
use crate::registry;
use crate::validate;
use crate::whitelist;
use std::path::{Path, PathBuf};

/// 决定本次要扫描的包管理器集合。
/// 空列表 = 全部已探测到的；未知 id 直接拒绝（防止把任意字符串当命令名）。
pub fn resolve_targets(request: &ScanRequest, detected: &[ManagerInfo]) -> AppResult<Vec<String>> {
    if request.managers.is_empty() {
        return Ok(detected.iter().filter(|m| m.detected).map(|m| m.id.clone()).collect());
    }
    for id in &request.managers {
        packages::ensure_known(id)?;
    }
    Ok(request.managers.clone())
}

/// 拉取某个管理器的已安装包
fn collect_packages(id: &str, req: &ScanRequest) -> Vec<PackageRecord> {
    match id {
        "npm" | "pnpm" | "yarn" | "bun" => {
            let (_, records) = packages::node_global(id, req.timeout_ms, req.measure_package_size);
            records
        }
        "pip" | "uv" => {
            let (_, records) = packages::python_packages(id, req.timeout_ms, req.measure_package_size);
            records
        }
        "cargo" => {
            let mut records = packages::cargo_packages(req.timeout_ms);
            if req.measure_package_size {
                for r in records.iter_mut() {
                    if let Some(p) = &r.path {
                        let pb = PathBuf::from(p);
                        if pb.is_dir() {
                            r.size = Some(fsutil::dir_size(&pb));
                        }
                    }
                }
            }
            records
        }
        "go" => {
            let root = manager::global_root_for("go", req.timeout_ms);
            packages::go_packages(root.as_deref(), req.measure_package_size)
        }
        "gem" => packages::gem_packages(req.timeout_ms),
        _ => Vec::new(),
    }
}

/// 执行一次完整扫描。
///
/// `detected` 由调用方（command 层）从 `AppState` 取出后传入 —— 这样本函数不依赖
/// Tauri 的 `State` 生命周期，可以整体丢进 `spawn_blocking`。
pub fn run_scan(detected: &[ManagerInfo], request: &ScanRequest) -> AppResult<ScanReport> {
    let started = std::time::Instant::now();
    let timeout = request.timeout_ms.clamp(1_000, 120_000);
    let targets = resolve_targets(request, detected)?;

    // 1. 汇总包列表
    let mut records: Vec<PackageRecord> = Vec::new();
    for id in &targets {
        let mut got = collect_packages(id, request);
        records.append(&mut got);
    }

    // 2. 冗余/旧版本识别
    packages::mark_old_versions(&mut records);
    records.sort_by(|a, b| a.manager.cmp(&b.manager).then_with(|| a.name.cmp(&b.name)));

    // 3. 缓存统计
    let mut caches: Vec<CacheStats> = Vec::new();
    for id in &targets {
        if let Ok(stat) = manager::cache_stats(id, timeout) {
            caches.push(stat);
        }
    }

    // 4. 组装报告
    let managers: Vec<ManagerInfo> =
        detected.iter().filter(|m| targets.contains(&m.id)).cloned().collect();
    let total_cache_bytes = caches.iter().map(|c| c.total_bytes).sum();
    let generated_at = chrono::Local::now().to_rfc3339();

    Ok(ScanReport {
        generated_at,
        hostname: std::env::var("COMPUTERNAME")
            .ok()
            .or_else(|| std::env::var("HOSTNAME").ok()),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        total_packages: records.len(),
        total_cache_bytes,
        duration_ms: started.elapsed().as_millis() as u64,
        managers,
        packages: records,
        caches,
    })
}

/// 清理前的候选枚举：与扫描共用的收集逻辑，但不统计体积
pub fn collect_candidates(detected: &[ManagerInfo], timeout_ms: u64) -> AppResult<Vec<CleanCandidate>> {
    let req = ScanRequest { managers: Vec::new(), measure_package_size: false, timeout_ms };
    let targets = resolve_targets(&req, detected)?;
    let mut records: Vec<PackageRecord> = Vec::new();
    for id in &targets {
        records.extend(collect_for_clean(id, timeout_ms));
    }
    packages::mark_old_versions(&mut records);
    Ok(cleaner::enumerate(&records, timeout_ms))
}

/// 清理专用：只收集识别旧版本所需的包信息（不统计体积，速度优先）
pub fn collect_for_clean(id: &str, timeout_ms: u64) -> Vec<PackageRecord> {
    match id {
        "npm" | "pnpm" | "yarn" | "bun" => packages::node_global(id, timeout_ms, false).1,
        "pip" | "uv" => packages::python_packages(id, timeout_ms, false).1,
        "cargo" => packages::cargo_packages(timeout_ms),
        "gem" => packages::gem_packages(timeout_ms),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 报告导出
// ---------------------------------------------------------------------------

/// 导出报告到指定路径。格式由扩展名与 `format` 共同决定，`format` 优先。
pub fn export(report: &ScanReport, request: &ExportRequest) -> AppResult<String> {
    let target = validate::expand_tilde(&request.target_path);
    if !target.is_absolute() {
        return Err(crate::error::AppError::invalid("导出路径必须是绝对路径"));
    }
    let format = request.format.to_ascii_lowercase();
    let content = match format.as_str() {
        "json" => serde_json::to_string_pretty(report)
            .map_err(|e| crate::error::AppError::internal(format!("序列化失败: {e}")))?,
        "csv" => to_csv(report),
        "markdown" | "md" => to_markdown(report),
        other => return Err(crate::error::AppError::invalid(format!("不支持的导出格式: {other}"))),
    };
    fsutil::write_text(&target, &content)?;
    Ok(target.to_string_lossy().to_string())
}

/// CSV 转义：字段含逗号/引号/换行时用双引号包裹并转义内部引号
fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn to_csv(report: &ScanReport) -> String {
    let mut out = String::from("包管理器,包名,版本,作用域,安装路径,体积(字节),冗余,说明\n");
    for p in &report.packages {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_escape(&p.manager),
            csv_escape(&p.name),
            csv_escape(p.version.as_deref().unwrap_or("")),
            csv_escape(&p.scope),
            csv_escape(p.path.as_deref().unwrap_or("")),
            p.size.map(|s| s.to_string()).unwrap_or_default(),
            if p.redundant { "是" } else { "否" },
            csv_escape(p.redundant_reason.as_deref().unwrap_or("")),
        ));
    }
    out.push('\n');
    out.push_str("缓存目录,路径,体积(字节),文件数\n");
    for c in &report.caches {
        out.push_str(&format!(
            "{},{},{},{}\n",
            csv_escape(&c.manager_id),
            csv_escape(&c.path),
            c.total_bytes,
            c.file_count
        ));
    }
    out
}

fn to_markdown(report: &ScanReport) -> String {
    let mut out = String::new();
    out.push_str("# PackInspect 扫描报告\n\n");
    out.push_str(&format!("- 生成时间：{}\n", report.generated_at));
    out.push_str(&format!("- 主机：{}\n", report.hostname.as_deref().unwrap_or("未知")));
    out.push_str(&format!("- 系统：{}\n", report.os));
    out.push_str(&format!("- 包总数：{}\n", report.total_packages));
    out.push_str(&format!("- 缓存总占用：{} ({})\n", fsutil::human_bytes(report.total_cache_bytes), report.total_cache_bytes));
    out.push_str(&format!("- 扫描耗时：{} ms\n\n", report.duration_ms));

    out.push_str("## 包管理器\n\n| 管理器 | 语言 | 已安装 | 版本 | 全局目录 | 缓存目录 |\n|---|---|---|---|---|---|\n");
    for m in &report.managers {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            m.name,
            m.language,
            if m.detected { "是" } else { "否" },
            m.version.as_deref().unwrap_or("-"),
            m.global_root.as_deref().unwrap_or("-"),
            m.cache_dir.as_deref().unwrap_or("-"),
        ));
    }

    out.push_str("\n## 缓存占用\n\n| 管理器 | 路径 | 占用 | 文件数 |\n|---|---|---|---|\n");
    for c in &report.caches {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            c.manager_id,
            c.path,
            fsutil::human_bytes(c.total_bytes),
            c.file_count
        ));
    }

    out.push_str("\n## 已安装包\n\n| 管理器 | 包名 | 版本 | 作用域 | 冗余 |\n|---|---|---|---|---|\n");
    for p in &report.packages {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            p.manager,
            p.name,
            p.version.as_deref().unwrap_or("-"),
            p.scope,
            if p.redundant { "是" } else { "" }
        ));
    }

    out.push_str("\n## 镜像源\n\n");
    for m in &report.managers {
        if let Some(reg) = &m.registry {
            out.push_str(&format!("**{}**", m.name));
            if let Some(cfg) = &m.config_file {
                out.push_str(&format!("（配置：`{cfg}`）"));
            }
            out.push('\n');
            for e in &reg.entries {
                out.push_str(&format!(
                    "- `{}` = `{}`{}{}\n",
                    e.key,
                    e.value,
                    e.hint.as_ref().map(|h| format!(" — {h}")).unwrap_or_default(),
                    if e.user_defined { "" } else { "（默认值）" }
                ));
            }
            out.push('\n');
        }
    }
    out
}

/// 供前端「打开所在文件夹」使用：只返回目录，不执行任何命令
pub fn parent_dir_of(path: &str) -> Option<String> {
    Path::new(path).parent().map(|p| p.to_string_lossy().to_string())
}

/// 简易诊断信息，用于「关于」面板
pub fn diagnostics() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "home": validate::home_dir().map(|p| p.to_string_lossy().to_string()),
        "supportedManagers": whitelist::MANAGERS.iter().map(|m| m.id).collect::<Vec<_>>(),
        "allowedOps": whitelist::MANAGERS.iter()
            .map(|m| (m.id.to_string(), whitelist::allowed_ops(m.id)))
            .collect::<std::collections::HashMap<_, _>>(),
        "executorAvailable": executor::resolve_executable(&["cmd.exe", "cmd"]).is_some(),
    })
}

/// 读取镜像源（对外入口，便于单独刷新某个管理器）
pub fn read_registry(id: &str, timeout_ms: u64) -> AppResult<RegistryConfig> {
    packages::ensure_known(id)?;
    registry::read(id, timeout_ms.clamp(1_000, 60_000))
}
