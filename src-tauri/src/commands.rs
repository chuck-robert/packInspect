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

/// 探测失败时的兜底条目：字段齐全但标记为未检测到，并附上失败原因。
///
/// 抽出来是因为并发探测与兜底路径都要用它，重复构造容易漏字段。
fn fallback_info(def: &crate::whitelist::ManagerDef, theme: icons::Theme, reason: String) -> ManagerInfo {
    ManagerInfo {
        id: def.id.to_string(),
        name: def.name.to_string(),
        language: def.language.to_string(),
        tier: def.tier,
        platforms: crate::whitelist::platform_label(def.platforms).to_string(),
        platform_applicable: crate::whitelist::platform_applies(def.platforms),
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
        warnings: vec![reason],
    }
}

/// 并发探测的上限。
///
/// 为什么是 6：每个管理器的探测都要起一到两个子进程（`--version` 等）。
/// 全量是 39 个管理器，如果无限制地并发：
/// - 会瞬间创建上百个进程，老机器的调度压力很大
/// - 注册表 PATH 回退、`cargo --version` 这类本身较重的命令会互相拖慢，
///   反而更容易撞上各自的超时
///
/// 6 是"能明显缩短总耗时、又不至于让单条命令变慢"的折中。串行 39 个的
/// 总耗时基本是逐个相加；6 路并发后接近"最慢的若干条之和"。
const DETECT_CONCURRENCY: usize = 6;

/// 探测所有已知包管理器（**并发**，单个失败不影响整体）。
///
/// 串行改并发的动机：首屏要等所有管理器探测完才出内容，
/// 而每个管理器都要起进程跑版本命令，串行等于把等待时间全加起来。
///
/// 结果按 `MANAGERS` 的原始顺序返回（用下标直接写入预分配数组），
/// 不依赖线程完成顺序 —— 否则侧边栏顺序会随机变动，看起来很乱。
fn detect_all(timeout_ms: u64, theme: icons::Theme) -> AppResult<Vec<ManagerInfo>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let defs = &crate::whitelist::MANAGERS;
    let total = defs.len();

    // 每个下标恰好被一个线程写入，因此这里不能简单用 Vec::with_capacity
    // （未初始化的槽位在 Rust 里无法安全占位）。先填兜底值，再按下标覆盖。
    //
    // 用 Mutex 而不是把 out 直接 move 进多个闭包：闭包各自持有 &mut out 会被
    // 借用检查器拒绝，而 Mutex 能把这个"按下标写入互不重叠"的事实表达出来
    // （写锁只在赋值那一瞬间持有，不覆盖探测过程）。
    let out: Mutex<Vec<ManagerInfo>> = Mutex::new(
        defs.iter()
            .map(|def| fallback_info(def, theme, "尚未探测".to_string()))
            .collect(),
    );

    let next = AtomicUsize::new(0);
    let workers = DETECT_CONCURRENCY.min(total);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    // fetch_add 让各线程自然地领取下一个下标，无需预先分片
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= total {
                        break;
                    }
                    let def = &defs[i];
                    // 探测阶段不读镜像源（改由独立命令按需加载，加快首屏）。
                    // 注意：探测本身**不持锁**，否则并发就退化成串行了。
                    let info = match manager::detect(def.id, timeout_ms, false, theme) {
                        Ok(info) => info,
                        Err(e) => fallback_info(def, theme, format!("探测失败: {}", e.message)),
                    };
                    if let Ok(mut guard) = out.lock() {
                        guard[i] = info;
                    }
                }
            });
        }
    });

    // 锁中毒只可能来自上面某次写入时 panic，此时返回兜底数据也比整体失败好
    Ok(out.into_inner().unwrap_or_else(|e| e.into_inner()))
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

#[cfg(test)]
mod detect_timing {
    use super::*;
    use std::time::Instant;

    /// 仅供对照：改动前的串行实现。
    ///
    /// 保留它不是为了回退，而是为了**随时能量化并发带来的收益** ——
    /// 否则"优化了性能"只是一句话，没法验证。
    fn detect_all_serial(timeout_ms: u64, theme: icons::Theme) -> Vec<ManagerInfo> {
        crate::whitelist::MANAGERS
            .iter()
            .map(|def| {
                manager::detect(def.id, timeout_ms, false, theme)
                    .unwrap_or_else(|e| fallback_info(def, theme, format!("探测失败: {}", e.message)))
            })
            .collect()
    }

    /// 计时对比：串行 vs 并发。
    ///
    /// 需要真实执行版本命令，因此标 `#[ignore]`，只在想量化时手动跑：
    ///
    /// ```text
    /// cargo test --lib detect_timing -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "需要真实执行版本命令，用于手工量化性能"]
    fn detect_timing_serial_vs_parallel() {
        let theme = icons::Theme::Dark;

        // 先串行
        let t0 = Instant::now();
        let serial = detect_all_serial(20_000, theme);
        let serial_ms = t0.elapsed().as_millis();

        // 再并发
        let t1 = Instant::now();
        let parallel = detect_all(20_000, theme).expect("并发探测应成功");
        let parallel_ms = t1.elapsed().as_millis();

        let detected = parallel.iter().filter(|m| m.detected).count();
        println!("\n=== 探测耗时对比 ===");
        println!("  管理器总数      : {}", parallel.len());
        println!("  检测到          : {detected}");
        println!("  串行            : {serial_ms} ms");
        println!("  并发（上限 {DETECT_CONCURRENCY}） : {parallel_ms} ms");
        if parallel_ms > 0 {
            println!(
                "  加速比          : {:.2}x",
                serial_ms as f64 / parallel_ms as f64
            );
        }

        // 并发版必须与串行版给出**同样数量、同样顺序**的结果 ——
        // 并发只该改耗时，不该改语义。
        assert_eq!(
            serial.len(),
            parallel.len(),
            "并发与串行的结果数量必须一致"
        );
        let serial_ids: Vec<&str> = serial.iter().map(|m| m.id.as_str()).collect();
        let parallel_ids: Vec<&str> = parallel.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(serial_ids, parallel_ids, "结果顺序必须与定义顺序一致");

        // 检测结论也应一致（同一台机器同一时刻，判定不该变）
        let serial_detected: Vec<&str> = serial
            .iter()
            .filter(|m| m.detected)
            .map(|m| m.id.as_str())
            .collect();
        let parallel_detected: Vec<&str> = parallel
            .iter()
            .filter(|m| m.detected)
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(
            serial_detected, parallel_detected,
            "并发与串行检测到的管理器集合必须一致"
        );
    }
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

/// 读取上次的磁盘快照（首屏秒开用）。
///
/// **不触发任何探测**，纯粹把上次保存的状态读出来给界面。
/// 返回 `null` 表示没有可用快照（首次运行、快照损坏、结构版本不匹配），
/// 前端此时就照常等真实探测。
///
/// 注意这是同步命令且很快（读一个 JSON 文件），因此不必 `spawn_blocking`。
#[tauri::command]
pub fn load_snapshot() -> Option<crate::snapshot::Snapshot> {
    crate::snapshot::load()
}

/// 保存当前状态为磁盘快照。
///
/// 由前端在探测结束 / 扫描结束后调用，存的就是它**正在显示**的那份数据。
/// 失败不报错（返回 false）：快照只是加速手段，写不进去最多下次启动慢一点，
/// 不该因此给用户弹错误。
#[tauri::command]
pub fn save_snapshot(managers: Vec<ManagerInfo>, report: Option<ScanReport>) -> bool {
    let snapshot = crate::snapshot::Snapshot {
        schema: crate::snapshot::SNAPSHOT_SCHEMA,
        captured_at: chrono::Local::now().to_rfc3339(),
        managers,
        report,
    };
    crate::snapshot::save(&snapshot)
}

/// 删除磁盘快照（用户主动"重新探测"时丢掉旧数据）
#[tauri::command]
pub fn clear_snapshot() -> AppResult<()> {
    crate::snapshot::clear()
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
pub async fn browse_packages(request: BrowseRequest) -> AppResult<BrowseResult> {
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
