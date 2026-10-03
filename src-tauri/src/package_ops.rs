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

use crate::console;
use crate::error::{AppError, AppResult};
use crate::executor;
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
///
/// **在可见的命令行窗口里执行**（见 `console` 模块）：
/// 安装过程有下载进度、依赖解析与报错，用户需要实时看到；
/// 只给一个转圈图标然后突然弹结果，卡住时完全无法判断发生了什么。
/// 同时把输出 tee 一份到日志，执行完由界面回显并给出日志路径。
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
    let timeout = timeout_for(op);
    let log_path = log_path_for(manager, package, op);

    // 故意**不等待按键**（pause=false）：
    // 否则后台的等待线程会一直挂到这个窗口被关闭为止，一旦用户走开就会撞上超时并被误判为失败。
    // 命令行的完整输出已经落进日志，界面会给出日志路径，用户随时可以回看。
    let outcome = console::run_visible(&exe, &args, &log_path, timeout, false)?;

    // 输出可能很长（cargo 编译日志），console 模块已按字符边界截断。
    // 先取出失败提示，再移动 stdout（否则会 borrow-after-move）
    let failure_hint = outcome.failure_hint();
    let stdout = outcome.stdout;
    let stderr = outcome.stderr;

    let message = if outcome.timed_out {
        Some(format!(
            "执行超时（{}秒）已被终止，日志可能不完整；请查看命令行窗口或日志文件",
            timeout.as_secs()
        ))
    } else if outcome.success {
        Some(format!("{package} {} 完成", op.as_str()))
    } else {
        Some(format!("命令失败：{failure_hint}"))
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
        log_path: Some(log_path.to_string_lossy().to_string()),
    })
}

/// 操作日志的存放位置：`%APPDATA%\PackInspect\logs\<管理器>-<操作>-<包名>.log`
///
/// 放在 APPDATA 而不是程序目录：程序可能装在只读位置，而日志要能随时写。
/// 文件名里的包名已过 `validate::package_name`（无 shell 元字符，也无路径分隔符），
/// 因此不会出现路径穿越。
fn log_path_for(manager: &str, package: &str, op: PackageOp) -> std::path::PathBuf {
    // settings_path 返回 Result<PathBuf>，取不到时退回临时目录（日志仍要能写）
    let base = crate::settings::settings_path()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("logs");
    let _ = std::fs::create_dir_all(&dir);

    // 包名里的 @ / + 之类保留，但把可能造成歧义的字符替换掉
    let safe: String = package
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    dir.join(format!("{manager}-{}-{safe}.log", op.as_str()))
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
        println!(
            "[install] stdout({} 字符): {}",
            installed.stdout.trim().chars().count(),
            installed.stdout.trim()
        );
        // 输出必须真的被回传到结果里（用于界面回显），不能是空字符串
        assert!(
            !installed.stdout.trim().is_empty(),
            "安装输出应回传到结果中，供界面回显"
        );
        assert!(installed.log_path.as_deref().is_some_and(|p| p.ends_with(".log")));
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

    /// 遍历白名单，保证每个**针对具体包**的操作模板都恰好有一个 `{}` 占位符。
    ///
    /// 为什么只查 update/uninstall/install：这三个是「对某个包做点什么」，
    /// 缺占位符就意味着它会作用于**全部**包（例如 `Pkg.update()` 而不带包名），
    /// 与右键菜单「更新此包」的语义严重不符，必须拦住。
    ///
    /// `updateIndex`（刷新索引）与 `outdated`（查询可升级）**刻意不在此列** ——
    /// 它们操作的是元数据，本来就不接受包名。`listGlobal` 同理。
    ///
    /// 反过来，若某个模板里出现了 `{}` 却不属于上述三类，也说明写错了位置。
    #[test]
    fn every_package_op_template_has_exactly_one_placeholder() {
        for def in whitelist::MANAGERS {
            for op in ["update", "uninstall", "install"] {
                if let Some(template) = whitelist::op_args(def.id, op) {
                    let count = template.iter().filter(|a| a.contains("{}")).count();
                    assert_eq!(
                        count, 1,
                        "{} 的 {} 模板必须恰好一个占位符（0 个会作用于全部包，多个会错位）",
                        def.id, op
                    );
                }
            }
            // 元数据类操作不该出现占位符
            for op in ["updateIndex", "outdated", "listGlobal", "version"] {
                if let Some(template) = whitelist::op_args(def.id, op) {
                    let count = template.iter().filter(|a| a.contains("{}")).count();
                    assert_eq!(
                        count, 0,
                        "{} 的 {} 是元数据/查询类操作，不该有占位符",
                        def.id, op
                    );
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

    /// 日志路径必须落在 APPDATA 下的 logs 目录，且不能因包名而产生路径穿越
    #[test]
    fn log_path_is_safe_for_odd_package_names() {
        let path = log_path_for("npm", "@scope/pkg", PackageOp::Install);
        let text = path.to_string_lossy().to_string();
        assert!(text.contains("logs"), "应放进 logs 目录: {text}");
        assert!(text.ends_with(".log"));
        assert!(text.contains("npm-install-"), "{text}");
        // 作用域包名里的 / 必须被替换，不能形成子目录
        assert!(!text.contains("scope/") && !text.contains("scope\\"), "不得产生子目录: {text}");

        let uninstall = log_path_for("pip", "requests", PackageOp::Uninstall);
        assert!(uninstall.to_string_lossy().contains("pip-uninstall-requests.log"));
    }
}
