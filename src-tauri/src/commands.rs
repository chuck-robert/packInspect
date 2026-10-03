//! Tauri command 层：前端唯一的入口。
//!
//! 约定：
//! - 所有命令都**不接收命令行字符串**，只接收结构化参数（manager id / 配置键 / 绝对路径）
//! - 耗时命令内部走 `spawn_blocking`，避免阻塞 Tauri 的 IPC 线程池
//! - 返回值统一 `Result<T, AppError>`，错误带 code 便于前端分支处理

use crate::actions;
use crate::error::{AppError, AppResult};
use crate::icons;
use crate::manager;
use crate::models::*;
use crate::package_ops;
use crate::packages;
use crate::plugins;
use crate::registry;
use crate::report;
use crate::settings;
use crate::validate;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 全局状态：缓存探测结果，避免每次扫描都重跑 `--version`
#[derive(Default)]
pub struct AppState {
    cache: Mutex<Option<(Instant, String, Vec<ManagerInfo>)>>,
    /// 包管理器 logo 缓存：按 (manager, theme) 只生成一次 SVG
    logos: icons::LogoCache,
}

/// 探测结果缓存有效期。版本号在会话内基本不变，5 分钟足够。
const DETECT_TTL: Duration = Duration::from_secs(300);

impl AppState {
    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.cache.lock() {
            *guard = None;
        }
    }

    /// 带缓存的全量探测（主题参与缓存键，避免切主题后拿到旧配色）
    pub fn detected_managers(&self, timeout_ms: u64, theme: icons::Theme) -> AppResult<Vec<ManagerInfo>> {
        let cache_key = theme.as_str();
        if let Ok(guard) = self.cache.lock() {
            if let Some((at, key, list)) = guard.as_ref() {
                if at.elapsed() < DETECT_TTL && key == cache_key {
                    return Ok(list.clone());
                }
            }
        }
        let list = detect_all(timeout_ms, theme)?;
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some((Instant::now(), cache_key.to_string(), list.clone()));
        }
        Ok(list)
    }
}

/// 探测所有已知包管理器（单个失败不影响整体）
fn detect_all(timeout_ms: u64, theme: icons::Theme) -> AppResult<Vec<ManagerInfo>> {
    let mut out = Vec::with_capacity(crate::whitelist::MANAGERS.len());
    for def in crate::whitelist::MANAGERS {
        // 探测阶段不读镜像源（改由独立命令按需加载，加快首屏）
        match manager::detect(def.id, timeout_ms, false, theme) {
            Ok(info) => out.push(info),
            Err(e) => out.push(ManagerInfo {
                id: def.id.to_string(),
                name: def.name.to_string(),
                language: def.language.to_string(),
                tier: def.tier,
                detected: false,
                version: None,
                exe_path: None,
                global_root: None,
                cache_dir: None,
                config_file: None,
                registry: None,
                logo: Some(icons::manager_logo_svg(def.id, def.name, theme)),
                download_url: Some(def.download_url.to_string()),
                docs_url: Some(def.docs_url.to_string()),
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
fn snapshot(
    state: &tauri::State<'_, AppState>,
    timeout_ms: u64,
    theme: icons::Theme,
) -> AppResult<Vec<ManagerInfo>> {
    state.detected_managers(timeout_ms, theme)
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
///
/// `theme` 决定管理器 logo 的配色变体（"dark" / "light"）。
#[tauri::command]
pub async fn detect_managers(
    state: tauri::State<'_, AppState>,
    force: Option<bool>,
    timeout_ms: Option<u64>,
    theme: Option<String>,
) -> AppResult<Vec<ManagerInfo>> {
    let timeout = clamp_timeout(timeout_ms, 20_000);
    let theme = icons::Theme::parse(theme.as_deref().unwrap_or("dark"));
    if force.unwrap_or(false) {
        state.invalidate();
        return blocking(move || detect_all(timeout, theme)).await;
    }
    let list = snapshot(&state, timeout, theme)?;
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
    theme: Option<String>,
) -> AppResult<HashMap<String, RegistryConfig>> {
    let timeout = clamp_timeout(timeout_ms, 20_000);
    let theme = icons::Theme::parse(theme.as_deref().unwrap_or("dark"));
    let detected = snapshot(&state, timeout, theme)?;
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
    let detected = snapshot(&state, req.timeout_ms.clamp(1_000, 120_000), icons::Theme::Dark)?;
    blocking(move || report::run_scan(&detected, &req)).await
}

/// 单个管理器的缓存占用统计
#[tauri::command]
pub async fn get_cache_stats(manager_id: String) -> AppResult<CacheStats> {
    packages::ensure_known(&manager_id)?;
    blocking(move || manager::cache_stats(&manager_id, 30_000)).await
}

/// 扫描**单个**管理器。
///
/// 供前端做渐进式扫描：每扫完一个就渲染一个，用户不必等最慢的那个。
/// 失败不抛错，而是返回 `ok = false` + `reason`，让前端标失败后继续。
#[tauri::command]
pub async fn scan_manager(
    manager_id: String,
    measure_package_size: Option<bool>,
    timeout_ms: Option<u64>,
) -> AppResult<ManagerScanResult> {
    packages::ensure_known(&manager_id)?;
    let timeout = clamp_timeout(timeout_ms, 30_000);
    let measure = measure_package_size.unwrap_or(false);
    blocking(move || Ok(report::scan_manager(&manager_id, timeout, measure))).await
}

/// 枚举清理候选（只读，不会删除任何东西）
#[tauri::command]
pub async fn list_clean_candidates(
    state: tauri::State<'_, AppState>,
    timeout_ms: Option<u64>,
    theme: Option<String>,
) -> AppResult<Vec<CleanCandidate>> {
    let timeout = clamp_timeout(timeout_ms, 30_000);
    let theme = icons::Theme::parse(theme.as_deref().unwrap_or("dark"));
    let detected = snapshot(&state, timeout, theme)?;
    blocking(move || report::collect_candidates(&detected, timeout)).await
}

/// 执行清理。`dry_run` 默认 true —— 前端必须显式传 false 才会真删。
#[tauri::command]
pub async fn clean_caches(
    state: tauri::State<'_, AppState>,
    request: CleanRequest,
    timeout_ms: Option<u64>,
    theme: Option<String>,
) -> AppResult<Vec<CleanResult>> {
    let timeout = clamp_timeout(timeout_ms, 30_000);
    let theme = icons::Theme::parse(theme.as_deref().unwrap_or("dark"));
    for id in &request.candidate_ids {
        validate::candidate_id(id)?;
    }
    let detected = snapshot(&state, timeout, theme)?;
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

// ---------------------------------------------------------------------------
// 包管理器 logo / 在线浏览 / 安装方案 / 管理动作 / 包内子节点
// ---------------------------------------------------------------------------

/// 批量取包管理器品牌 logo（按主题，走缓存）。
///
/// 切换主题后前端调用一次即可拿到全部配色变体，不必重新探测。
#[tauri::command]
pub fn manager_logos(
    state: tauri::State<'_, AppState>,
    request: LogoRequest,
) -> AppResult<Vec<LogoResponse>> {
    let theme = icons::Theme::parse(&request.theme);
    let targets: Vec<(&'static str, &'static str)> = if request.managers.is_empty() {
        crate::whitelist::MANAGERS.iter().map(|m| (m.id, m.name)).collect()
    } else {
        let mut list = Vec::new();
        for id in &request.managers {
            let def = crate::whitelist::find(id)
                .ok_or_else(|| AppError::invalid(format!("未知包管理器: {id}")))?;
            list.push((def.id, def.name));
        }
        list
    };

    Ok(targets
        .into_iter()
        .map(|(id, name)| LogoResponse {
            manager_id: id.to_string(),
            data_uri: state.logos.get_or_create(id, name, theme),
        })
        .collect())
}

/// 在包仓库里搜索可安装的新包
#[tauri::command]
pub async fn browse_packages(request: BrowseRequest) -> AppResult<Vec<RemotePackage>> {
    packages::ensure_known(&request.manager)?;
    let timeout = clamp_timeout(request.timeout_ms, 20_000);
    blocking(move || {
        let mut req = request;
        req.timeout_ms = Some(timeout);
        crate::browse::browse(&req)
    })
    .await
}

/// 生成安装方案（**只返回命令，不执行**）
#[tauri::command]
pub fn plan_install(manager_id: String, package: String) -> AppResult<InstallPlan> {
    packages::ensure_known(&manager_id)?;
    crate::browse::install_plan(&manager_id, &package)
}

/// 执行真实的包管理操作（更新 / 卸载 / 安装）。
///
/// 【这是本项目唯一会改动用户环境的入口】
/// 安全约束逐条：
/// 1. 操作名先经 `PackageOp::parse` 收敛为三个字面量之一；
/// 2. 包管理器与包名分别过白名单与 `validate::package_name`（拒绝一切 shell 元字符）；
/// 3. 参数来自 `whitelist::op_args` 的**静态数组模板**，只把 `{}` 替换为已校验的包名；
/// 4. `confirm` 必须为 true —— 前端必须完成二次确认才能走到这里；
/// 5. 强制超时（装包可能很慢，默认 5 分钟，上限 10 分钟）；
/// 6. 不经过 shell：Windows 上 .cmd 由 executor 统一包装，其余直接 spawn。
#[tauri::command]
pub async fn run_package_op(
    manager_id: String,
    package: String,
    action: String,
    confirm: bool,
) -> AppResult<PackageOpResult> {
    if !confirm {
        return Err(AppError::forbidden("需要先确认才能执行该操作"));
    }
    let op = validate::PackageOp::parse(&action)?;
    let template = validate::resolve_package_op(&manager_id, &package, op)?;

    blocking(move || package_ops::run(&manager_id, &package, op, template)).await
}

/// 生成某个包支持的右键管理动作。
///
/// 一期只有 `manage` / `inspect` / `openDocs` 为可用状态；
/// 更新、卸载、安装会返回**等价官方命令**但 `enabled = false`，由界面标注为占位。
#[tauri::command]
pub fn package_actions(
    manager_id: String,
    package: String,
    scope: Option<String>,
) -> AppResult<Vec<ManagementAction>> {
    packages::ensure_known(&manager_id)?;
    validate::package_name(&package)?;
    Ok(actions::actions_for(&manager_id, &package, scope.as_deref().unwrap_or("global")))
}

/// 展开包内子节点（插件 / 扩展 / 依赖 / 文件）
#[tauri::command]
pub async fn package_plugins(
    request: PluginsRequest,
) -> AppResult<Vec<PluginNode>> {
    let timeout = clamp_timeout(request.timeout_ms, 20_000);
    blocking(move || {
        plugins::collect_checked(
            &request.manager,
            &request.package,
            request.path,
            request.version,
            timeout,
        )
    })
    .await
}

/// 未检测到的包管理器 + 官方下载入口（满足「没有就提示去官网下载」）
#[tauri::command]
pub async fn install_hints(state: tauri::State<'_, AppState>) -> AppResult<Vec<InstallHint>> {
    let detected = snapshot(&state, 20_000, icons::Theme::Dark)?;
    Ok(report::install_hints(&detected))
}

// ---------------------------------------------------------------------------
// 外部链接 / 设置
// ---------------------------------------------------------------------------

/// 用系统默认浏览器打开链接。
///
/// 只接受 https 且主机在白名单内的地址 —— 前端无法借此打开任意 URL。
#[tauri::command]
pub async fn open_external_link(request: OpenLinkRequest) -> AppResult<String> {
    let url = match request.kind.as_str() {
        // kind = manager：由后端从白名单定义里取官方地址，前端不能自带 URL
        "manager" => {
            let def = crate::whitelist::find(&request.target)
                .ok_or_else(|| AppError::invalid(format!("未知包管理器: {}", request.target)))?;
            def.download_url.to_string()
        }
        "docs" => {
            let def = crate::whitelist::find(&request.target)
                .ok_or_else(|| AppError::invalid(format!("未知包管理器: {}", request.target)))?;
            def.docs_url.to_string()
        }
        "url" => request.target.clone(),
        other => return Err(AppError::invalid(format!("不支持的链接类型: {other}"))),
    };
    blocking(move || settings::open_external(&url)).await
}

/// 读取持久化设置
#[tauri::command]
pub fn get_settings() -> AppSettings {
    settings::load()
}

/// 保存设置
#[tauri::command]
pub async fn save_settings(settings_data: AppSettings) -> AppResult<String> {
    blocking(move || settings::save(&settings_data)).await
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
