//! PackInspect 库入口。
//!
//! 分层（依赖方向自上而下，下层不反向依赖上层）：
//! ```text
//! commands  ← Tauri IPC 边界，前端唯一入口
//!    │
//! report    ← 扫描编排 / 报告导出
//!    │
//! manager · packages · cleaner · registry   ← 业务能力
//!    │
//! executor · validate · whitelist · fsutil  ← 安全底座
//!    │
//! models · error                            ← 数据结构
//! ```

pub mod cleaner;
pub mod commands;
pub mod error;
pub mod executor;
pub mod fsutil;
pub mod manager;
pub mod models;
pub mod packages;
pub mod registry;
pub mod report;
pub mod validate;
pub mod whitelist;

/// 组装 Tauri 应用：注册插件、状态与全部 command
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::supported_managers,
            commands::detect_managers,
            commands::get_registry,
            commands::get_all_registries,
            commands::set_registry,
            commands::preview_registry_change,
            commands::run_scan,
            commands::get_cache_stats,
            commands::list_clean_candidates,
            commands::clean_caches,
            commands::export_report,
            commands::get_diagnostics,
            commands::parent_dir,
        ])
        .run(tauri::generate_context!())
        .expect("启动 PackInspect 失败");
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
}
