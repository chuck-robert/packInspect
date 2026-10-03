//! Tauri command 层：前端唯一的入口。
//!
//! 约定：
//! - 所有命令都**不接收命令行字符串**，只接收结构化参数（manager id / 配置键 / 绝对路径）
//! - 耗时命令内部走 `spawn_blocking`，避免阻塞 Tauri 的 IPC 线程池
//! - 返回值统一 `Result<T, AppError>`，错误带 code 便于前端分支处理

use crate::error::{AppError, AppResult};
use crate::manager;
use crate::models::*;
use crate::packages;
use crate::registry;
use crate::report;
use crate::validate;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 全局状态：缓存探测结果，避免每次扫描都重跑 `--version`
#[derive(Default)]
pub struct AppState {
    cache: Mutex<Option<(Instant, Vec<ManagerInfo>)>>,
}

/// 探测结果缓存有效期。版本号在会话内基本不变，5 分钟足够。
const DETECT_TTL: Duration = Duration::from_secs(300);

impl AppState {
    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.cache.lock() {
            *guard = None;
        }
    }

    /// 带缓存的全量探测
    pub fn detected_managers(&self, timeout_ms: u64) -> AppResult<Vec<ManagerInfo>> {
        if let Ok(guard) = self.cache.lock() {
            if let Some((at, list)) = guard.as_ref() {
                if at.elapsed() < DETECT_TTL {
                    return Ok(list.clone());
                }
            }
        }
        let list = detect_all(timeout_ms)?;
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some((Instant::now(), list.clone()));
        }
        Ok(list)
    }
}

/// 探测所有已知包管理器（单个失败不影响整体）
fn detect_all(timeout_ms: u64) -> AppResult<Vec<ManagerInfo>> {
    let mut out = Vec::with_capacity(crate::whitelist::MANAGERS.len());
    for def in crate::whitelist::MANAGERS {
        // 探测阶段不读镜像源（改由独立命令按需加载，加快首屏）
        match manager::detect(def.id, timeout_ms, false) {
            Ok(info) => out.push(info),
            Err(e) => out.push(ManagerInfo {
                id: def.id.to_string(),
                name: def.name.to_string(),
                language: def.language.to_string(),
                detected: false,
                version: None,
                exe_path: None,
                global_root: None,
                cache_dir: None,
                config_file: None,
                registry: None,
                warnings: vec![format!("探测失败: {}", e.message)],
            }),
        }
    }
    Ok(out)
}

/// 把可能很慢的同步逻辑丢到阻塞线程池
async fn blocking<T, F>(f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::internal(format!("后台任务异常: {e}")))?
}

/// 从 State 取出探测结果后立即释放 State 借用，便于后续 move 进闭包
fn snapshot(state: &tauri::State<'_, AppState>, timeout_ms: u64) -> AppResult<Vec<ManagerInfo>> {
    state.detected_managers(timeout_ms)
}

fn clamp_timeout(value: Option<u64>, default: u64) -> u64 {
    value.unwrap_or(default).clamp(1_000, 120_000)
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------

/// 列出所有受支持的包管理器定义（纯静态信息，用于首屏骨架渲染）
#[tauri::command]
pub fn supported_managers() -> Vec<serde_json::Value> {
    crate::whitelist::MANAGERS
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "language": m.language,
                "allowedOps": crate::whitelist::allowed_ops(m.id),
            })
        })
        .collect()
}

/// 探测本机包管理器。`force = true` 时忽略缓存重新探测。
#[tauri::command]
pub async fn detect_managers(
    state: tauri::State<'_, AppState>,
    force: Option<bool>,
    timeout_ms: Option<u64>,
) -> AppResult<Vec<ManagerInfo>> {
    let timeout = clamp_timeout(timeout_ms, 20_000);
    if force.unwrap_or(false) {
        state.invalidate();
        return blocking(move || detect_all(timeout)).await;
    }
    let list = snapshot(&state, timeout)?;
    Ok(list)
}

/// 读取指定管理器的镜像源配置
#[tauri::command]
pub async fn get_registry(manager_id: String, timeout_ms: Option<u64>) -> AppResult<RegistryConfig> {
    packages::ensure_known(&manager_id)?;
    let timeout = clamp_timeout(timeout_ms, 20_000);
    blocking(move || report::read_registry(&manager_id, timeout)).await
}

/// 读取所有已探测管理器的镜像源
#[tauri::command]
pub async fn get_all_registries(
    state: tauri::State<'_, AppState>,
    timeout_ms: Option<u64>,
) -> AppResult<HashMap<String, RegistryConfig>> {
    let timeout = clamp_timeout(timeout_ms, 20_000);
    let detected = snapshot(&state, timeout)?;
    let ids: Vec<String> = detected.iter().filter(|m| m.detected).map(|m| m.id.clone()).collect();
    blocking(move || {
        let mut map = HashMap::new();
        for id in ids {
            if let Ok(cfg) = registry::read(&id, timeout) {
                map.insert(id, cfg);
            }
        }
        Ok(map)
    })
    .await
}

/// 写回镜像源配置（会先备份原文件）
#[tauri::command]
pub async fn set_registry(manager_id: String, key: String, value: String) -> AppResult<String> {
    packages::ensure_known(&manager_id)?;
    blocking(move || registry::write(&manager_id, &key, &value)).await
}

/// 预览镜像源修改后的配置内容（不落盘），用于前端 diff 展示
#[tauri::command]
pub fn preview_registry_change(
    manager_id: String,
    key: String,
    value: String,
    original: String,
) -> AppResult<String> {
    packages::ensure_known(&manager_id)?;
    if !validate::config_key_allowed(&manager_id, &key) {
        return Err(AppError::forbidden(format!("不允许修改配置项: {key}")));
    }
    if value.contains(['\n', '\r']) {
        return Err(AppError::forbidden("配置值不能包含换行"));
    }
    if key == "registry" || key.ends_with(":registry") || key.contains("url") {
        validate::registry_url(&value)?;
    }
    Ok(registry::preview(&manager_id, &original, &key, &value))
}

/// 主扫描入口
#[tauri::command]
pub async fn run_scan(
    state: tauri::State<'_, AppState>,
    request: Option<ScanRequest>,
) -> AppResult<ScanReport> {
    let req = request.unwrap_or_default();
    let detected = snapshot(&state, req.timeout_ms.clamp(1_000, 120_000))?;
    blocking(move || report::run_scan(&detected, &req)).await
}

/// 单个管理器的缓存占用统计
#[tauri::command]
pub async fn get_cache_stats(manager_id: String) -> AppResult<CacheStats> {
    packages::ensure_known(&manager_id)?;
    blocking(move || manager::cache_stats(&manager_id, 30_000)).await
}

/// 枚举清理候选（只读，不会删除任何东西）
#[tauri::command]
pub async fn list_clean_candidates(
    state: tauri::State<'_, AppState>,
    timeout_ms: Option<u64>,
) -> AppResult<Vec<CleanCandidate>> {
    let timeout = clamp_timeout(timeout_ms, 30_000);
    let detected = snapshot(&state, timeout)?;
    blocking(move || report::collect_candidates(&detected, timeout)).await
}

/// 执行清理。`dry_run` 默认 true —— 前端必须显式传 false 才会真删。
#[tauri::command]
pub async fn clean_caches(
    state: tauri::State<'_, AppState>,
    request: CleanRequest,
    timeout_ms: Option<u64>,
) -> AppResult<Vec<CleanResult>> {
    let timeout = clamp_timeout(timeout_ms, 30_000);
    for id in &request.candidate_ids {
        validate::candidate_id(id)?;
    }
    let detected = snapshot(&state, timeout)?;
    blocking(move || {
        let candidates = report::collect_candidates(&detected, timeout)?;
        crate::cleaner::run(&candidates, &request.candidate_ids, request.dry_run, timeout)
    })
    .await
}

/// 导出扫描报告到用户选择的路径
#[tauri::command]
pub async fn export_report(report_data: ScanReport, request: ExportRequest) -> AppResult<String> {
    blocking(move || report::export(&report_data, &request)).await
}

/// 系统诊断信息（关于面板）
#[tauri::command]
pub fn get_diagnostics() -> serde_json::Value {
    report::diagnostics()
}

/// 返回路径的父目录（纯字符串运算，不执行任何 shell）
#[tauri::command]
pub fn parent_dir(path: String) -> AppResult<String> {
    report::parent_dir_of(&path).ok_or_else(|| AppError::invalid("该路径没有父目录"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_clamped_to_sane_range() {
        assert_eq!(clamp_timeout(Some(10), 20_000), 1_000);
        assert_eq!(clamp_timeout(Some(u64::MAX), 20_000), 120_000);
        assert_eq!(clamp_timeout(None, 20_000), 20_000);
    }
}
