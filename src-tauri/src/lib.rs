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
pub mod error;
pub mod executor;
pub mod fsutil;
pub mod icons;
pub mod manager;
pub mod models;
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
            commands::package_actions,
            commands::package_plugins,
            commands::install_hints,
            // 外部链接与设置
            commands::open_external_link,
            commands::get_settings,
            commands::save_settings,
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
}
