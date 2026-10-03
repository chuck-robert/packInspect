//! 真实包管理操作的执行层。
//!
//! 【本项目唯一会改动用户环境的模块】
//!
//! 之所以单独成文件：这是全部安全约束的收口点，值得能被一眼审查完。
//!
//! 执行前的前置校验（在 `commands::run_package_op` 完成）：
//!   1. `confirm` 必须为 true —— 前端必须走完二次确认
//!   2. 操作名经 `PackageOp::parse` 收敛为 update / uninstall / install 三个字面量
//!   3. 包管理器必须在白名单内；包名必须过 `validate::package_name`
//!      （只允许 `[A-Za-z0-9@._/+-~]`，拒绝 `;` `|` `$` 反引号、换行等一切 shell 元字符）
//!   4. 参数来自 `whitelist::op_args` 的**静态数组模板**，本模块只做一次
//!      「把模板里唯一的 `{}` 替换成已校验包名」的操作 ——
//!      前端无法追加、插入或改写任何参数
//!
//! 执行时：不经过 shell（`executor` 统一处理 Windows 的 .cmd 包装），带强制超时。

use crate::error::{AppError, AppResult};
use crate::executor::{self, ExecRequest};
use crate::models::PackageOpResult;
use crate::validate::PackageOp;
use crate::whitelist;
use std::time::Duration;

/// 安装类操作可能很慢（下依赖、编译），因此给足超时
fn timeout_for(op: PackageOp) -> Duration {
    match op {
        // 卸载通常很快
        PackageOp::Uninstall => Duration::from_secs(120),
        // 更新/安装要下载与解包；cargo 还可能要编译
        PackageOp::Update | PackageOp::Install => Duration::from_secs(300),
    }
}

/// 把静态模板渲染成参数数组。
///
/// `template` 由白名单给出，其中恰有一个元素含 `{}`。
/// 这里断言这一点，避免将来有人写入含多个占位符的模板造成参数错位。
fn render_args(template: &[&str], package: &str) -> AppResult<Vec<String>> {
    let placeholders = template.iter().filter(|arg| arg.contains("{}")).count();
    if placeholders != 1 {
        return Err(AppError::internal(format!(
            "操作模板必须恰好含一个 {{}} 占位符，实际为 {placeholders} 个"
        )));
    }
    Ok(template
        .iter()
        .map(|arg| arg.replace("{}", package))
        .collect())
}

/// 生成给用户看的等价命令（用于日志与结果回显，不用于执行）
fn display_command(manager: &str, args: &[String]) -> String {
    let is_powershell = manager == "powershellget";
    let quoted: Vec<String> = args
        .iter()
        .map(|arg| {
            // PowerShell 的 -Command 参数含空格，需要引号才能复制粘贴运行
            if is_powershell && arg.contains(' ') {
                format!("\"{arg}\"")
            } else {
                arg.clone()
            }
        })
        .collect();
    let program = if is_powershell { "pwsh" } else { manager };
    format!("{program} {}", quoted.join(" "))
}

/// 执行一个包管理操作
pub fn run(
    manager: &str,
    package: &str,
    op: PackageOp,
    template: &[&str],
) -> AppResult<PackageOpResult> {
    let def = whitelist::find(manager)
        .ok_or_else(|| AppError::invalid(format!("不支持的包管理器: {manager}")))?;

    let exe = executor::resolve_executable(def.exe_candidates)
        .ok_or_else(|| AppError::not_installed(manager))?;

    let args = render_args(template, package)?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let request = ExecRequest::new(exe.to_string_lossy().to_string(), &arg_refs)
        .with_timeout_ms(timeout_for(op).as_millis() as u64);

    let outcome = executor::run_resolved(&exe, &request)?;

    // 命令输出可能很长（cargo 编译日志），截断后再回传，避免撑爆 IPC
    let stdout = truncate(&outcome.stdout, 8_000);
    let stderr = truncate(&outcome.stderr, 8_000);

    let message = if outcome.timed_out {
        Some(format!("执行超时（{}秒），操作可能仍在后台进行", timeout_for(op).as_secs()))
    } else if outcome.success {
        Some(format!("{package} {} 完成", op.as_str()))
    } else {
        Some(format!("命令失败：{}", outcome.failure_hint()))
    };

    Ok(PackageOpResult {
        manager_id: manager.to_string(),
        package: package.to_string(),
        action: op.as_str().to_string(),
        command: display_command(manager, &args),
        success: outcome.success,
        timed_out: outcome.timed_out,
        exit_code: outcome.exit_code,
        stdout,
        stderr,
        message,
        duration_ms: outcome.duration_ms,
    })
}

/// 按字符边界截断（避免切断多字节字符产生乱码）
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}\n… （输出已截断）")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate;

    #[test]
    fn render_args_replaces_the_single_placeholder() {
        let template = &["update", "-g", "{}"];
        let args = render_args(template, "vue").unwrap();
        assert_eq!(args, vec!["update", "-g", "vue"]);
    }

    /// **真实执行的端到端验证**（会真的装包与卸包，因此默认忽略）。
    ///
    /// 为什么必须做这一步：模板正确 ≠ 命令能跑通。路径解析、参数顺序、
    /// 退出码判断、输出回传都要在真实进程里才验证得到。
    ///
    /// 刻意选择**可逆且极小**的包，并在结束时卸载，不留副作用：
    /// ```text
    /// cargo test --lib npm_install_uninstall_round_trip -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "会真实安装/卸载 npm 包；用 --ignored 显式运行"]
    fn npm_install_uninstall_round_trip() {
        const PKG: &str = "is-number"; // 零依赖、极小、不会影响任何东西

        // 前置：npm 必须可用，否则跳过（而不是失败）
        let Some(exe) = executor::resolve_executable(&["npm.cmd", "npm.exe", "npm"]) else {
            println!("跳过：未找到 npm");
            return;
        };
        let _ = exe;

        // ---- 1. 安装 ----
        let template = validate::resolve_package_op("npm", PKG, PackageOp::Install)
            .expect("npm install 应可用");
        let installed = run("npm", PKG, PackageOp::Install, template).expect("执行不应 panic");
        println!("[install] success={} exit={:?}", installed.success, installed.exit_code);
        println!("[install] command={}", installed.command);
        println!("[install] stdout={}", installed.stdout.chars().take(400).collect::<String>());
        assert!(installed.success, "安装应成功：{}", installed.stderr);
        assert_eq!(installed.action, "install");
        assert_eq!(installed.package, PKG);
        assert!(
            installed.command.contains(PKG),
            "回显命令里应包含包名：{}",
            installed.command
        );
        assert!(installed.exit_code == Some(0), "退出码应为 0");
        assert!(!installed.timed_out, "不应超时");

        // ---- 2. 卸载（把环境还原）----
        let template = validate::resolve_package_op("npm", PKG, PackageOp::Uninstall)
            .expect("npm uninstall 应可用");
        let removed = run("npm", PKG, PackageOp::Uninstall, template).expect("执行不应 panic");
        println!("[uninstall] success={} exit={:?}", removed.success, removed.exit_code);
        assert!(removed.success, "卸载应成功：{}", removed.stderr);
        assert_eq!(removed.action, "uninstall");
    }

    /// 对不存在的包执行安装，应**失败但不 panic**，并把错误如实回传
    #[test]
    #[ignore = "需要网络；用 --ignored 显式运行"]
    fn install_of_nonexistent_package_fails_gracefully() {
        let template = validate::resolve_package_op(
            "npm",
            "this-package-definitely-does-not-exist-packinspect",
            PackageOp::Install,
        )
        .expect("模板应存在");
        let result = run(
            "npm",
            "this-package-definitely-does-not-exist-packinspect",
            PackageOp::Install,
            template,
        )
        .expect("即使失败也不应 panic");

        println!("[missing] success={} message={:?}", result.success, result.message);
        assert!(!result.success, "不存在的包不应安装成功");
        assert!(result.exit_code.is_some(), "应有退出码");
        assert!(
            !result.stderr.is_empty() || !result.stdout.is_empty(),
            "应回传输出供用户排查"
        );
    }

    #[test]
    fn render_args_rejects_templates_with_wrong_placeholder_count() {
        // 0 个占位符
        assert!(render_args(&["update", "-g"], "vue").is_err());
        // 2 个占位符
        assert!(render_args(&["update", "{}", "{}"], "vue").is_err());
    }

    /// 包名在进入本模块前已被校验，但这里再确认一次「注入字符串不会静默通过」
    #[test]
    fn injection_package_names_are_rejected_upstream() {
        for bad in ["vue; rm -rf /", "$(id)", "`id`", "a b", "../../x"] {
            assert!(
                validate::resolve_package_op("npm", bad, PackageOp::Install).is_err(),
                "{bad:?} 应被拒绝"
            );
        }
    }

    #[test]
    fn every_op_template_has_exactly_one_placeholder() {
        // 遍历白名单，确保每个 update/uninstall/install 模板都恰好一个 {}
        for def in whitelist::MANAGERS {
            for op in ["update", "uninstall", "install"] {
                if let Some(template) = whitelist::op_args(def.id, op) {
                    let count = template.iter().filter(|a| a.contains("{}")).count();
                    assert_eq!(count, 1, "{} 的 {} 模板占位符数量应为 1", def.id, op);
                }
            }
        }
    }

    #[test]
    fn display_command_quotes_powershell_commands() {
        let args = vec!["-Command".to_string(), "Install-Module -Name Pester -Force".to_string()];
        let shown = display_command("powershellget", &args);
        assert!(shown.contains("\"Install-Module -Name Pester -Force\""), "{shown}");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let text = "中文中文中文";
        let cut = truncate(text, 3);
        assert!(cut.starts_with("中文中"));
        assert!(cut.contains("已截断"));
        // 未超长时原样返回
        assert_eq!(truncate("short", 100), "short");
    }
}
