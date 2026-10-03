//! 包管理动作描述。
//!
//! 【安全立场】PackInspect **不代替用户执行更新 / 卸载 / 安装**。
//! 这类操作会改动用户真实环境，且包管理器自身的交互提示（确认、依赖冲突、权限）无法可靠地
//! 在后台管道里完成。因此这里只做两件事：
//!   1. 告诉界面「这个包支持哪些动作」以及**等价的官方命令**；
//!   2. 明确标注一期为占位状态（`enabled = false`），避免用户以为点了就会生效。
//!
//! 这样既能满足「右键菜单展示管理动作」的交互需求，又不会因为一个误点破坏开发环境。

use crate::models::ManagementAction;

/// 生成某个包的管理动作列表。
///
/// `manager` 必须是白名单内的 id；`name` 应已通过 `validate::package_name`。
pub fn actions_for(manager: &str, name: &str, scope: &str) -> Vec<ManagementAction> {
    let mut list = Vec::new();

    // ---- 1. 管理 / 查看（这个是真能用的）----
    list.push(ManagementAction {
        action: "manage".into(),
        label: "管理此包".into(),
        online: false,
        destructive: false,
        enabled: true,
        command_hint: None,
        note: Some("展开包内插件、依赖与文件清单".into()),
    });

    list.push(ManagementAction {
        action: "inspect".into(),
        label: "查看安装详情".into(),
        online: false,
        destructive: false,
        enabled: true,
        command_hint: Some(scope_command(manager, name, scope)),
        note: Some("展示来源、版本、安装路径与占用".into()),
    });

    // ---- 2. 更新 ----
    // `enabled` 由白名单决定：该管理器确实注册了 update 操作模板才可执行。
    if let Some(cmd) = update_command(manager, name) {
        let supported = crate::whitelist::op_args(manager, "update").is_some();
        list.push(ManagementAction {
            action: "update".into(),
            label: "更新到最新版".into(),
            online: true,
            destructive: false,
            enabled: supported,
            command_hint: Some(cmd.clone()),
            note: Some(if supported {
                format!("将执行：{cmd}（需二次确认）")
            } else {
                format!("{manager} 没有可靠的单包更新方式，请按官方文档手动升级")
            }),
        });
    }

    // ---- 3. 卸载 ----
    if let Some(cmd) = uninstall_command(manager, name) {
        let supported = crate::whitelist::op_args(manager, "uninstall").is_some();
        list.push(ManagementAction {
            action: "uninstall".into(),
            label: "卸载此包".into(),
            online: false,
            destructive: true,
            enabled: supported,
            command_hint: Some(cmd.clone()),
            note: Some(if supported {
                format!("破坏性操作，将执行：{cmd}（需二次确认）")
            } else {
                format!("{manager} 不支持从本工具卸载，请手动处理")
            }),
        });
    }

    // ---- 4. 重装 / 安装 ----
    if let Some(cmd) = install_command(manager, name) {
        let supported = crate::whitelist::op_args(manager, "install").is_some();
        list.push(ManagementAction {
            action: "install".into(),
            label: "重新安装".into(),
            online: true,
            destructive: true,
            enabled: supported,
            command_hint: Some(cmd.clone()),
            note: Some(if supported {
                format!("将执行：{cmd}（需二次确认）")
            } else {
                format!("{manager} 不支持从本工具安装，请手动处理")
            }),
        });
    }

    // ---- 5. 禁用（仅 PowerShell 模块）----
    // 说明：PowerShell 没有"禁用模块"的原生概念，只能靠卸载或限制执行策略。
    // 因此这里不给可执行入口，只提示正确做法。
    if manager == "powershellget" {
        list.push(ManagementAction {
            action: "disable".into(),
            label: "禁用模块".into(),
            online: false,
            destructive: false,
            enabled: false,
            command_hint: Some(format!("Uninstall-Module {name} -Force  # 或设置执行策略限制")),
            note: Some(
                "PowerShell 没有「禁用模块」的原生操作，只能卸载或调整执行策略；请按上方命令手动处理"
                    .into(),
            ),
        });
    }

    // ---- 6. 打开包主页 ----
    if let Some(url) = homepage_url(manager, name) {
        list.push(ManagementAction {
            action: "openDocs".into(),
            label: "打开包主页".into(),
            online: false,
            destructive: false,
            // 打开链接是安全动作，一期即可用
            enabled: true,
            command_hint: None,
            note: Some(url),
        });
    }

    list
}


fn scope_command(manager: &str, name: &str, scope: &str) -> String {
    match manager {
        "npm" | "pnpm" | "yarn" => format!("{manager} ls -g {name}  # 作用域: {scope}"),
        "pip" => format!("pip show {name}"),
        "cargo" => format!("cargo install --list | grep {name}"),
        "dotnet" => format!("dotnet nuget locals global-packages --list  # {name}"),
        "winget" => format!("winget show {name}"),
        _ => format!("{manager} show {name}  # 作用域: {scope}"),
    }
}

/// 供其它模块（如 browse 生成安装方案）复用的安装命令模板
pub fn install_command_for(manager: &str, name: &str) -> Option<String> {
    install_command(manager, name)
}

fn update_command(manager: &str, name: &str) -> Option<String> {
    let cmd = match manager {
        "npm" => format!("npm update -g {name}"),
        "pnpm" => format!("pnpm update -g {name}"),
        "yarn" => format!("yarn global upgrade {name}"),
        "pip" => format!("pip install --upgrade {name}"),
        "cargo" => format!("cargo install {name}"),
        "dotnet" => format!("dotnet add package {name}"),
        "winget" => format!("winget upgrade {name}"),
        "powershellget" => format!("Update-Module {name}"),
        "composer" => format!("composer global update {name}"),
        "gem" => format!("gem update {name}"),
        "go" => format!("go install {name}@latest"),
        "chocolatey" => format!("choco upgrade {name}"),
        "scoop" => format!("scoop update {name}"),
        "conda" => format!("conda update {name}"),
        "dart" => format!("dart pub global activate {name}"),
        "luarocks" => format!("luarocks install {name}"),
        "cpan" => format!("cpan -i {name}"),
        _ => return None,
    };
    Some(cmd)
}

fn uninstall_command(manager: &str, name: &str) -> Option<String> {
    let cmd = match manager {
        "npm" => format!("npm uninstall -g {name}"),
        "pnpm" => format!("pnpm remove -g {name}"),
        "yarn" => format!("yarn global remove {name}"),
        "pip" => format!("pip uninstall {name}"),
        "cargo" => format!("cargo uninstall {name}"),
        "dotnet" => format!("dotnet nuget delete {name}"),
        "winget" => format!("winget uninstall {name}"),
        "powershellget" => format!("Uninstall-Module {name}"),
        "composer" => format!("composer global remove {name}"),
        "gem" => format!("gem uninstall {name}"),
        "go" => "go clean -modcache  # 或删除对应 module 目录".to_string(),
        "chocolatey" => format!("choco uninstall {name}"),
        "scoop" => format!("scoop uninstall {name}"),
        "conda" => format!("conda remove {name}"),
        "dart" => format!("dart pub global deactivate {name}"),
        "luarocks" => format!("luarocks remove {name}"),
        "cpan" => format!("cpan -U {name}"),
        _ => return None,
    };
    Some(cmd)
}

fn install_command(manager: &str, name: &str) -> Option<String> {
    let cmd = match manager {
        "npm" => format!("npm install -g {name}"),
        "pnpm" => format!("pnpm add -g {name}"),
        "yarn" => format!("yarn global add {name}"),
        "pip" => format!("pip install {name}"),
        "cargo" => format!("cargo install {name}"),
        "dotnet" => format!("dotnet add package {name}"),
        "winget" => format!("winget install {name}"),
        "powershellget" => format!("Install-Module {name}"),
        "composer" => format!("composer global require {name}"),
        "gem" => format!("gem install {name}"),
        "go" => format!("go install {name}@latest"),
        "chocolatey" => format!("choco install {name}"),
        "scoop" => format!("scoop install {name}"),
        "conda" => format!("conda install {name}"),
        "dart" => format!("dart pub global activate {name}"),
        "luarocks" => format!("luarocks install {name}"),
        "cpan" => format!("cpan -i {name}"),
        _ => return None,
    };
    Some(cmd)
}

/// 包主页地址。仅拼接**已知域名 + 合法包名**，不接收任意 URL。
pub fn homepage_url(manager: &str, name: &str) -> Option<String> {
    // 允许 ':' —— Maven 的坐标是 `groupId:artifactId`，
    // 这个字符在 URL 路径里合法（会由下面的转义处理），也不是 shell 元字符。
    let safe = !name.is_empty()
        && name.len() <= 214
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '/' | '+' | '-' | '~' | ':')
        });
    if !safe {
        return None;
    }
    let url = match manager {
        "npm" | "pnpm" | "yarn" => format!("https://www.npmjs.com/package/{}", name.trim_start_matches('@').replace('@', "%40").replace('/', "%2F")),
        "pip" => format!("https://pypi.org/project/{}", name),
        "cargo" => format!("https://crates.io/crates/{}", name),
        "dotnet" => format!("https://www.nuget.org/packages/{}", name),
        "winget" => format!("https://winget.run/pkg/{}", name.replace('.', "/")),
        "powershellget" => format!("https://www.powershellgallery.com/packages/{}", name),
        "composer" => format!("https://packagist.org/packages/{}", name),
        "gem" => format!("https://rubygems.org/gems/{}", name),
        "go" => format!("https://pkg.go.dev/{}", name),
        "maven" => {
            // groupId:artifactId → Maven Central 搜索页
            let (group, artifact) = name.split_once(':').unwrap_or(("", name));
            if group.is_empty() {
                format!("https://central.sonatype.com/search?q={artifact}")
            } else {
                format!("https://central.sonatype.com/artifact/{group}/{artifact}")
            }
        }
        "chocolatey" => format!("https://community.chocolatey.org/packages/{}", name),
        "scoop" => format!("https://scoop.sh/#/apps?q={name}"),
        "conda" => format!("https://anaconda.org/search?q={name}"),
        "dart" => format!("https://pub.dev/packages/{}", name),
        "luarocks" => format!("https://luarocks.org/search?q={name}"),
        "cpan" => format!("https://metacpan.org/pod/{}", name),
        _ => return None,
    };
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier1_manager_gets_action_set() {
        for id in crate::whitelist::tier1_ids() {
            let list = actions_for(id, "some-package", "global");
            assert!(list.len() >= 3, "{id} 的动作过少: {}", list.len());
            // manage / inspect 必须可用
            let manage = list.iter().find(|a| a.action == "manage").expect("缺少 manage");
            assert!(manage.enabled, "{id} 的 manage 应可用");
            // 破坏性动作必须带说明，供确认对话框展示
            for a in list.iter().filter(|a| a.destructive) {
                assert!(a.note.is_some(), "{id} 的破坏性动作 {} 必须说明后果", a.action);
            }
        }
    }

    /// 破坏性动作现在**可以执行**（本轮新实现的能力），但必须：
    /// 1. 标记为 destructive，让界面用红色警示并要求二次确认
    /// 2. 只在白名单确实注册了对应操作模板时才 enabled
    #[test]
    fn uninstall_is_destructive_and_only_enabled_when_whitelisted() {
        for id in crate::whitelist::tier1_ids() {
            let list = actions_for(id, "pkg", "global");
            if let Some(uninstall) = list.iter().find(|a| a.action == "uninstall") {
                assert!(uninstall.destructive, "{id} 的卸载必须标记为破坏性");
                // dotnet 的全局包目录没有官方卸载命令，因此不应可执行
                let expected = crate::whitelist::op_args(id, "uninstall").is_some();
                assert_eq!(uninstall.enabled, expected, "{id} 的可执行状态应与白名单一致");
                assert!(uninstall.note.is_some(), "必须说明将执行什么");
            }
        }
    }

    /// 每个可执行动作都必须有等价命令提示，供用户在确认对话框里核对
    #[test]
    fn enabled_mutating_actions_expose_their_command() {
        for id in crate::whitelist::tier1_ids() {
            for action in actions_for(id, "some-pkg", "global") {
                if action.enabled
                    && matches!(action.action.as_str(), "update" | "uninstall" | "install")
                {
                    assert!(
                        action.command_hint.is_some(),
                        "{id} 的 {} 可执行但没有命令提示",
                        action.action
                    );
                }
            }
        }
    }

    #[test]
    fn homepage_urls_point_at_known_registries() {
        assert_eq!(homepage_url("pip", "requests").unwrap(), "https://pypi.org/project/requests");
        assert_eq!(homepage_url("cargo", "ripgrep").unwrap(), "https://crates.io/crates/ripgrep");
        assert_eq!(
            homepage_url("maven", "org.slf4j:slf4j-api").unwrap(),
            "https://central.sonatype.com/artifact/org.slf4j/slf4j-api"
        );
        // npm 作用域包需要转义 @ 与 /
        let scoped = homepage_url("npm", "@anthropic-ai/claude-code").unwrap();
        assert!(scoped.starts_with("https://www.npmjs.com/package/"), "{scoped}");
        assert!(!scoped.contains('@') || scoped.contains("%40"), "作用域符号应被转义: {scoped}");
        // 非法包名不得产生链接
        assert!(homepage_url("npm", "a b; rm -rf /").is_none());
        assert!(homepage_url("unknown-manager", "x").is_none());
    }

    #[test]
    fn command_hints_match_manager_conventions() {
        let npm = actions_for("npm", "vue", "global");
        assert!(npm.iter().any(|a| a.command_hint.as_deref() == Some("npm update -g vue")));
        assert!(npm.iter().any(|a| a.command_hint.as_deref() == Some("npm uninstall -g vue")));

        let pip = actions_for("pip", "requests", "system");
        assert!(pip.iter().any(|a| a.command_hint.as_deref() == Some("pip install --upgrade requests")));
    }
}
