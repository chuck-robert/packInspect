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
    if let Some(cmd) = update_command(manager, name) {
        list.push(ManagementAction {
            action: "update".into(),
            label: "更新到最新版".into(),
            online: true,
            destructive: false,
            enabled: false,
            command_hint: Some(cmd),
            note: Some(placeholder_note(manager, "更新")),
        });
    }

    // ---- 3. 卸载 ----
    if let Some(cmd) = uninstall_command(manager, name) {
        list.push(ManagementAction {
            action: "uninstall".into(),
            label: "卸载此包".into(),
            online: false,
            destructive: true,
            enabled: false,
            command_hint: Some(cmd),
            note: Some(placeholder_note(manager, "卸载")),
        });
    }

    // ---- 4. 重装 / 安装 ----
    if let Some(cmd) = install_command(manager, name) {
        list.push(ManagementAction {
            action: "install".into(),
            label: "重新安装".into(),
            online: true,
            destructive: true,
            enabled: false,
            command_hint: Some(cmd),
            note: Some(placeholder_note(manager, "重装")),
        });
    }

    // ---- 5. 禁用（仅 PowerShell 模块等支持）----
    if manager == "powershellget" {
        list.push(ManagementAction {
            action: "disable".into(),
            label: "禁用模块".into(),
            online: false,
            destructive: false,
            enabled: false,
            command_hint: Some(format!("Uninstall-Module {name} -WhatIf")),
            note: Some(placeholder_note(manager, "禁用")),
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

/// 占位说明：明确告知用户当前不会真的执行
fn placeholder_note(manager: &str, verb: &str) -> String {
    format!("一期为占位按钮：{verb}会改动你的真实环境，PackInspect 只给出等价命令，不代为执行（{manager}）")
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
            // manage / inspect 必须一期可用
            let manage = list.iter().find(|a| a.action == "manage").expect("缺少 manage");
            assert!(manage.enabled, "{id} 的 manage 应一期可用");
            // 破坏性动作必须显式禁用
            for a in list.iter().filter(|a| a.destructive) {
                assert!(!a.enabled, "{id} 的破坏性动作 {} 不应默认可执行", a.action);
                assert!(a.note.is_some(), "占位动作必须说明原因");
            }
        }
    }

    #[test]
    fn uninstall_is_never_enabled() {
        for id in crate::whitelist::tier1_ids() {
            let list = actions_for(id, "pkg", "global");
            if let Some(uninstall) = list.iter().find(|a| a.action == "uninstall") {
                assert!(!uninstall.enabled, "{id} 的卸载不应可执行");
                assert!(uninstall.destructive);
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
