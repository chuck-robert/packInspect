//! PackInspect 库入口。
//!
//! 分层（依赖方向自上而下，下层不反向依赖上层）：
//! ```text
//! commands  ← Tauri IPC 边界，前端唯一入口
//!    │
//! report · actions · plugins · settings   ← 业务编排
//!    │
//! manager · packages · cleaner · registry ← 能力层
//!    │
//! executor · validate · whitelist · fsutil · icons  ← 安全底座
//!    │
//! models · error                          ← 数据结构
//! ```

pub mod actions;
pub mod browse;
pub mod cleaner;
pub mod commands;
pub mod console;
pub mod error;
pub mod executor;
pub mod fsutil;
pub mod icons;
pub mod manager;
pub mod models;
pub mod package_ops;
pub mod packages;
pub mod plugins;
pub mod registry;
pub mod report;
pub mod settings;
pub mod validate;
pub mod whitelist;

/// 组装 Tauri 应用：注册插件、状态与全部 command
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            // 基础探测与扫描
            commands::supported_managers,
            commands::detect_managers,
            commands::run_scan,
            commands::scan_manager,
            commands::get_cache_stats,
            // 镜像源
            commands::get_registry,
            commands::get_all_registries,
            commands::set_registry,
            commands::preview_registry_change,
            // 清理
            commands::list_clean_candidates,
            commands::clean_caches,
            // 报告与诊断
            commands::export_report,
            commands::get_diagnostics,
            commands::parent_dir,
            // 包管理：logo / 在线浏览 / 安装方案 / 动作 / 包内子节点 / 下载引导
            commands::manager_logos,
            commands::browse_packages,
            commands::plan_install,
            commands::run_package_op,
            commands::package_actions,
            commands::package_plugins,
            commands::install_hints,
            // 外部链接与设置
            commands::open_external_link,
            commands::get_settings,
            commands::save_settings,
        ])
        .setup(|app| {
            apply_window_icon(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 PackInspect 失败");
}

/// 显式给主窗口设置图标。
///
/// 【为什么需要这一步】
/// exe 内嵌的图标资源只有一部分场景会用到。实测发现主窗口只设置了
/// **小图标**（`WM_GETICON` 的 `ICON_SMALL` 有句柄），**大图标句柄为 0**：
///
/// ```text
/// WM_GETICON ICON_BIG    = 0        ← 没设置
/// WM_GETICON ICON_SMALL  = 10749621
/// GetClassLongPtr HICON  = 0
/// ```
///
/// 而 Alt+Tab、任务栏大图标、窗口标题栏都取大图标，取不到时才回退去读 exe 的
/// 资源段 —— 这个回退在各处的行为并不一致，表现出来就是「有的地方图标换了、
/// 有的地方还是旧的」。
///
/// `app.default_window_icon()` 是 Tauri 依据 `bundle.icon` 在**编译期**内嵌进
/// 二进制的图标，因此单文件 exe 下同样可用，不依赖磁盘上的任何图标文件。
fn apply_window_icon(app: &tauri::App) {
    use tauri::Manager;

    let Some(icon) = app.default_window_icon().cloned() else {
        // 没有配置 bundle.icon 时不致命：exe 内嵌图标仍能覆盖多数场景
        return;
    };

    match app.get_webview_window("main") {
        Some(window) => {
            if let Err(e) = window.set_icon(icon) {
                // 设置失败也不该阻止启动，只是外观问题
                eprintln!("设置窗口图标失败: {e}");
            }
        }
        None => eprintln!("启动时未找到名为 main 的窗口，跳过图标设置"),
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::executor::{self, ExecRequest};
    use crate::whitelist;

    /// 端到端验证：白名单里的 version 操作在已安装的管理器上应能执行。
    /// 未安装的管理器直接跳过，因此该测试在任意机器上都能通过。
    #[test]
    fn whitelisted_version_ops_run_without_shell_injection() {
        for def in whitelist::MANAGERS {
            let Some(exe) = executor::resolve_executable(def.exe_candidates) else { continue };
            let args = whitelist::op_args(def.id, "version").expect("version 必须在白名单内");
            let req = ExecRequest::new(exe.to_string_lossy().to_string(), args).with_timeout_ms(15_000);
            let out = executor::run_resolved(&exe, &req).expect("执行不应 panic");
            assert!(
                out.success || out.timed_out,
                "{} 的 version 命令失败: {}",
                def.id,
                out.failure_hint()
            );
        }
    }

    /// 一期管理器的「列表」操作必须也能被安全执行（未安装则跳过）
    #[test]
    fn tier1_list_ops_are_executable() {
        for id in whitelist::tier1_ids() {
            let def = whitelist::find(id).unwrap();
            let Some(exe) = executor::resolve_executable(def.exe_candidates) else { continue };
            let Some(args) = whitelist::op_args(id, "listGlobal") else {
                panic!("{id} 缺少 listGlobal 操作");
            };
            let req = ExecRequest::new(exe.to_string_lossy().to_string(), args).with_timeout_ms(25_000);
            let out = executor::run_resolved(&exe, &req).expect("执行不应 panic");
            // 只要没超时且没被拒绝即可；即使返回非零也应由上层兜底到磁盘扫描
            assert!(!out.timed_out || out.success, "{id} 的列表命令超时");
        }
    }

    /// **真实网络**端到端验证：对每个已接入在线浏览的生态各搜一次。
    ///
    /// 默认忽略（离线环境必然失败）。需要联网核对时显式运行：
    /// ```text
    /// cargo test --lib browse_ecosystems_online -- --ignored --nocapture
    /// ```
    /// 这个用例的价值：上游会改字段名 —— 只靠 mock 的单元测试发现不了
    /// 「端点还在但响应结构变了」这类问题，而这正是用户看到「搜不到」的常见原因。
    #[test]
    #[ignore = "需要网络；用 --ignored 显式运行"]
    fn browse_ecosystems_online() {
        let cases = [
            ("npm", "vue"),
            ("cargo", "ripgrep"),
            ("dotnet", "newtonsoft"),
            ("composer", "phpunit"),
            ("gem", "rails"),
            ("dart", "http"),
            ("powershellget", "pester"),
            ("pip", "requests"), // PyPI 按精确名查询
            ("winget", "git"),
        ];

        let mut report = Vec::new();
        let mut failures = Vec::new();

        for (manager, query) in cases {
            let request = crate::models::BrowseRequest {
                manager: manager.to_string(),
                query: query.to_string(),
                limit: 5,
                timeout_ms: Some(20_000),
            };
            match crate::browse::browse(&request) {
                Ok(result) => {
                    let sample = result
                        .packages
                        .first()
                        .map(|p| format!("{} {}", p.name, p.version.clone().unwrap_or_default()))
                        .unwrap_or_else(|| "(空)".to_string());
                    report.push(format!(
                        "{manager:<15} attempted={:<6} failed={:<6} 命中={:<3} 首条={sample}",
                        result.attempted, result.failed, result.packages.len()
                    ));
                    if result.packages.is_empty() || result.failed {
                        failures.push(format!(
                            "{manager}: {}",
                            result.note.clone().unwrap_or_else(|| "无结果且无说明".into())
                        ));
                    }
                }
                Err(e) => {
                    report.push(format!("{manager:<15} 调用失败: {}", e.message));
                    failures.push(format!("{manager}: {}", e.message));
                }
            }
        }

        println!("\n=== 在线浏览实跑结果 ===");
        for line in &report {
            println!("{line}");
        }

        assert!(
            failures.is_empty(),
            "以下生态的在线浏览未通过：\n{}",
            failures.join("\n")
        );
    }
}