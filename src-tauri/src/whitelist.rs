//! 包管理器注册表 + 命令白名单。
//!
//! 【安全核心】前端永远不能传命令行字符串，只能传 `{ manager, op }`。
//! 这里定义每个 (manager, op) 对应的**静态参数数组**，Rust 侧据此拼装 `Command`。
//! 参数里出现的一切动态值都必须先过 `validate` 校验。
//!
//! 支持范围按生态族划分，`tier` 只用于界面上的大致排序：
//! - 一期：pip / npm / pnpm / yarn / cargo / dotnet(nuget) / winget
//! - 二期：PowerShellGet / composer / gem / go / maven
//! - 三期：chocolatey / scoop / conda / dart(pub) / luarocks / cpan
//! - 四期：Gradle / vcpkg / Conan / bun / deno / pipx / opam / dub / nimble /
//!   cabal / stack / Julia / mix(Hex) 等语言与构建工具
//! - 系统级：brew / apt / pacman / dnf / yum / flatpak / snap / CocoaPods / SPM
//!   （这些**只在对应操作系统上存在**，见 `platforms`）

/// 一个包管理器的静态定义
pub struct ManagerDef {
    pub id: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    /// 优先级阶段：1 = 一期，2 = 二期，3 = 三期，4 = 四期
    pub tier: u8,
    /// **适用平台**，取值只能是下列之一：
    /// - `"all"`  全平台
    /// - `"win"`  仅 Windows
    /// - `"macos"` 仅 macOS
    /// - `"linux"` 仅 Linux
    /// - `"unix"`  macOS 与 Linux（不含 Windows）
    ///
    /// 为什么必须显式标注：`apt` / `pacman` / `brew` / `CocoaPods` 这类管理器
    /// 在 Windows 上**不可能存在**。若不标注，界面会显示「未在 PATH 中找到 apt」——
    /// 用户会以为是自己环境有问题，而不是"这东西本来就不在这个系统上"。
    pub platforms: &'static str,
    /// 候选可执行文件名（按优先级）。
    /// Node 生态写作 `xxx.cmd` 优先：Windows 上真正的入口是批处理，`.ps1` 无法被直接执行。
    pub exe_candidates: &'static [&'static str],
    /// 该管理器允许执行的操作 → 静态参数数组
    pub ops: &'static [(&'static str, &'static [&'static str])],
    /// 清理时**允许删除**的路径推导规则：相对于缓存根的路径片段
    pub cache_subdirs: &'static [&'static str],
    /// 是否保护整个缓存根（内容寻址仓库 / 巨型下载缓存，删了代价过大）
    pub cache_root_protected: bool,
    /// 官网地址：未安装时前端给出「前往下载」按钮
    pub download_url: &'static str,
    /// 官方文档 / 安装说明
    pub docs_url: &'static str,
}

/// 当前运行的操作系统代号，与 `ManagerDef::platforms` 的取值对应
pub const CURRENT_OS: &str = if cfg!(target_os = "windows") {
    "win"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

/// 该管理器的 `platforms` 声明是否适用于当前系统
pub fn platform_applies(platforms: &str) -> bool {
    match platforms {
        "all" => true,
        // unix 覆盖 macOS 与 Linux，但不含 Windows
        "unix" => cfg!(any(target_os = "macos", target_os = "linux")),
        other => other == CURRENT_OS,
    }
}

/// 给用户看的平台说明（用于「本系统不适用」的提示）
pub fn platform_label(platforms: &str) -> &'static str {
    match platforms {
        "all" => "全部平台",
        "win" => "仅 Windows",
        "macos" => "仅 macOS",
        "linux" => "仅 Linux",
        "unix" => "macOS / Linux",
        _ => "未知平台",
    }
}

/// 该管理器是否适用于当前系统
pub fn applies_here(id: &str) -> bool {
    find(id).map(|d| platform_applies(d.platforms)).unwrap_or(false)
}


pub static MANAGERS: &[ManagerDef] = &[
    // ======================= 一期 =======================
    ManagerDef {
        id: "npm",
        name: "npm",
        language: "node",
        tier: 1,
        platforms: "all",
        exe_candidates: &["npm.cmd", "npm.exe", "npm"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["ls", "-g", "--depth=0", "--json", "--long=false"]),
            ("rootGlobal", &["root", "-g"]),
            ("prefixGlobal", &["prefix", "-g"]),
            ("cacheDir", &["config", "get", "cache"]),
            ("globalConfigPath", &["config", "get", "globalconfig"]),
            ("outdatedGlobal", &["outdated", "-g", "--json", "--depth=0"]),
            ("update", &["update", "-g", "{}"]),
            ("uninstall", &["uninstall", "-g", "{}"]),
            ("install", &["install", "-g", "{}"]),
        ],
        cache_subdirs: &["_cacache", "_npx", "_logs", "_update-notifier"],
        cache_root_protected: false,
        download_url: "https://nodejs.org/en/download",
        docs_url: "https://docs.npmjs.com/cli/v10/commands/npm",
    },
    ManagerDef {
        id: "pnpm",
        name: "pnpm",
        language: "node",
        tier: 1,
        platforms: "all",
        exe_candidates: &["pnpm.cmd", "pnpm.exe", "pnpm"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["ls", "-g", "--depth=0", "--json"]),
            ("rootGlobal", &["root", "-g"]),
            ("storePath", &["store", "path"]),
            ("cacheDir", &["store", "path"]),
            ("outdatedGlobal", &["outdated", "-g", "--json"]),
            ("update", &["update", "-g", "{}"]),
            ("uninstall", &["remove", "-g", "{}"]),
            ("install", &["add", "-g", "{}"]),
        ],
        cache_subdirs: &["v3/files", "v10/files", "metadata", "metadata-full", "metadata-v1.3"],
        cache_root_protected: true,
        download_url: "https://pnpm.io/installation",
        docs_url: "https://pnpm.io/cli/install",
    },
    ManagerDef {
        id: "yarn",
        name: "yarn",
        language: "node",
        tier: 1,
        platforms: "all",
        exe_candidates: &["yarn.cmd", "yarn.exe", "yarn"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["global", "list", "--json", "--depth=0"]),
            ("rootGlobal", &["global", "dir"]),
            ("cacheDir", &["cache", "dir"]),
            ("update", &["global", "upgrade", "{}"]),
            ("uninstall", &["global", "remove", "{}"]),
            ("install", &["global", "add", "{}"]),
        ],
        cache_subdirs: &["v6", "v4"],
        cache_root_protected: false,
        download_url: "https://classic.yarnpkg.com/lang/en/docs/install/",
        docs_url: "https://classic.yarnpkg.com/lang/en/docs/cli/",
    },
    ManagerDef {
        id: "pip",
        name: "pip",
        language: "python",
        tier: 1,
        platforms: "all",
        exe_candidates: &["pip.exe", "pip3.exe", "pip", "pip3"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--format=json", "--disable-pip-version-check"]),
            ("outdatedGlobal", &["list", "--outdated", "--format=json", "--disable-pip-version-check"]),
            ("update", &["install", "--upgrade", "{}"]),
            ("uninstall", &["uninstall", "-y", "{}"]),
            ("install", &["install", "{}"]),
            ("cacheDir", &["cache", "dir"]),
        ],
        cache_subdirs: &["http", "http-v2", "wheels", "selfcheck"],
        cache_root_protected: false,
        download_url: "https://bootstrap.pypa.io/get-pip.py",
        docs_url: "https://pip.pypa.io/en/stable/cli/",
    },
    ManagerDef {
        id: "cargo",
        name: "cargo",
        language: "rust",
        tier: 1,
        platforms: "all",
        exe_candidates: &["cargo.exe", "cargo"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["install", "--list"]),
            ("update", &["install", "{}"]),
            ("uninstall", &["uninstall", "{}"]),
            ("install", &["install", "{}"]),
            ("cacheDir", &["--version"]),
        ],
        cache_subdirs: &["registry/cache", "registry/index", "git/db"],
        cache_root_protected: true,
        download_url: "https://rustup.rs/",
        docs_url: "https://doc.rust-lang.org/cargo/commands/cargo-install.html",
    },
    ManagerDef {
        id: "dotnet",
        name: "dotnet / NuGet",
        language: "dotnet",
        tier: 1,
        platforms: "all",
        exe_candidates: &["dotnet.exe", "dotnet"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["nuget", "list", "source"]),
            ("sdkList", &["--list-sdks"]),
            ("runtimeList", &["--list-runtimes"]),
            ("install", &["add", "package", "{}"]),
        ],
        // %USERPROFILE%\.nuget\packages → 一级子目录即包名
        cache_subdirs: &[],
        cache_root_protected: true,
        download_url: "https://dotnet.microsoft.com/download",
        docs_url: "https://learn.microsoft.com/nuget/consume-packages/managing-the-global-packages-folder-and-cache-folder",
    },
    ManagerDef {
        id: "winget",
        name: "winget",
        language: "windows",
        tier: 1,
        platforms: "win",
        exe_candidates: &["winget.exe", "winget"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--disable-interactivity"]),
            ("sourceList", &["source", "list"]),
            ("listUpgrades", &["upgrade", "--include-unknown", "--disable-interactivity"]),
            ("update", &["upgrade", "{}", "--disable-interactivity", "--accept-source-agreements", "--accept-package-agreements"]),
            ("uninstall", &["uninstall", "{}", "--disable-interactivity"]),
            ("install", &["install", "{}", "--disable-interactivity", "--accept-source-agreements", "--accept-package-agreements"]),        ],
        cache_subdirs: &["Cache", "Logs", "DownloadedInstallers"],
        cache_root_protected: false,
        download_url: "https://aka.ms/getwinget",
        docs_url: "https://learn.microsoft.com/windows/package-manager/winget/",
    },

    // ======================= 二期 =======================
    ManagerDef {
        id: "powershellget",
        name: "PowerShellGet",
        language: "powershell",
        tier: 2,
        platforms: "all",
        exe_candidates: &["pwsh.exe", "powershell.exe", "pwsh", "powershell"],
        ops: &[
            ("version", &["-NoProfile", "-NonInteractive", "-Command", "$PSVersionTable.PSVersion.ToString()"]),
            (
                "listGlobal",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Get-Module -ListAvailable | Sort-Object Name,Version -Unique | Select-Object Name,Version,ModuleBase | ConvertTo-Json -Compress",
                ],
            ),
            ("update", &["-NoProfile", "-NonInteractive", "-Command", "Update-Module -Name {} -Force"]),
            ("uninstall", &["-NoProfile", "-NonInteractive", "-Command", "Uninstall-Module -Name {} -Force"]),
            ("install", &["-NoProfile", "-NonInteractive", "-Command", "Install-Module -Name {} -Force -Scope CurrentUser"]),
        ],
        cache_subdirs: &[],
        cache_root_protected: true,
        download_url: "https://www.powershellgallery.com/",
        docs_url: "https://learn.microsoft.com/powershell/module/powershellget/",
    },
    ManagerDef {
        id: "composer",
        name: "composer",
        language: "php",
        tier: 2,
        platforms: "all",
        exe_candidates: &["composer.bat", "composer.phar", "composer"],
        ops: &[
            ("version", &["--version", "--no-ansi"]),
            ("listGlobal", &["global", "show", "--format=json", "--no-ansi"]),
            ("rootGlobal", &["global", "config", "home"]),
            ("cacheDir", &["config", "cache-dir", "--global"]),
            ("update", &["global", "update", "{}", "--no-ansi"]),
            ("uninstall", &["global", "remove", "{}", "--no-ansi"]),
            ("install", &["global", "require", "{}", "--no-ansi"]),
        ],
        cache_subdirs: &["cache", "files"],
        cache_root_protected: false,
        download_url: "https://getcomposer.org/download/",
        docs_url: "https://getcomposer.org/doc/03-cli.md",
    },
    ManagerDef {
        id: "gem",
        name: "gem",
        language: "ruby",
        tier: 2,
        platforms: "all",
        exe_candidates: &["gem.cmd", "gem.exe", "gem"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--local", "--no-versions"]),
            ("rootGlobal", &["environment", "home"]),
            ("cacheDir", &["environment", "home"]),
            ("outdatedGlobal", &["outdated", "--local"]),
            ("update", &["update", "{}"]),
            ("uninstall", &["uninstall", "-x", "-I", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["cache"],
        cache_root_protected: false,
        download_url: "https://rubyinstaller.org/downloads/",
        docs_url: "https://guides.rubygems.org/command-reference/",
    },
    ManagerDef {
        id: "go",
        name: "go mod",
        language: "go",
        tier: 2,
        platforms: "all",
        exe_candidates: &["go.exe", "go"],
        ops: &[
            ("version", &["version"]),
            ("listGlobal", &["version", "-m"]),
            ("cacheDir", &["env", "GOMODCACHE"]),
            ("goEnv", &["env"]),
            // go 没有"更新单个已安装模块"的概念，install 即更新
            ("update", &["install", "{}@latest"]),
            ("install", &["install", "{}@latest"]),
        ],
        cache_subdirs: &["cache/download"],
        cache_root_protected: true,
        download_url: "https://go.dev/dl/",
        docs_url: "https://go.dev/ref/mod",
    },
    ManagerDef {
        id: "maven",
        name: "maven",
        language: "java",
        tier: 2,
        platforms: "all",
        exe_candidates: &["mvn.cmd", "mvn.exe", "mvn"],
        ops: &[
            ("version", &["-v"]),
            ("listGlobal", &["-v"]),
        ],
        cache_subdirs: &["repository"],
        cache_root_protected: true,
        download_url: "https://maven.apache.org/download.cgi",
        docs_url: "https://maven.apache.org/guides/",
    },

    // ======================= 三期 =======================
    ManagerDef {
        id: "chocolatey",
        name: "Chocolatey",
        language: "windows",
        tier: 3,
        platforms: "win",
        exe_candidates: &["choco.exe", "choco"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--local-only", "--limit-output"]),
            ("update", &["upgrade", "{}", "-y"]),
            ("uninstall", &["uninstall", "{}", "-y"]),
            ("install", &["install", "{}", "-y"]),
            ("cacheDir", &["config", "get", "cacheLocation"]),
        ],
        cache_subdirs: &[],
        cache_root_protected: true,
        download_url: "https://chocolatey.org/install",
        docs_url: "https://docs.chocolatey.org/en-us/choco/commands/",
    },
    ManagerDef {
        id: "scoop",
        name: "Scoop",
        language: "windows",
        tier: 3,
        platforms: "win",
        exe_candidates: &["scoop.cmd", "scoop.ps1", "scoop"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list"]),
            ("update", &["update", "{}"]),
            ("uninstall", &["uninstall", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["cache", "buckets"],
        cache_root_protected: false,
        download_url: "https://scoop.sh/",
        docs_url: "https://github.com/ScoopInstaller/Scoop/wiki/Commands",
    },
    ManagerDef {
        id: "conda",
        name: "conda",
        language: "python",
        tier: 3,
        platforms: "all",
        exe_candidates: &["conda.exe", "conda.bat", "conda"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--json"]),
            ("infoJson", &["info", "--json"]),
            ("envList", &["env", "list", "--json"]),
            ("update", &["update", "{}", "-y"]),
            ("uninstall", &["remove", "{}", "-y"]),
            ("install", &["install", "{}", "-y"]),
        ],
        cache_subdirs: &["pkgs"],
        cache_root_protected: true,
        download_url: "https://docs.conda.io/en/latest/miniconda.html",
        docs_url: "https://docs.conda.io/projects/conda/en/latest/commands/index.html",
    },
    ManagerDef {
        id: "dart",
        name: "dart pub",
        language: "dart",
        tier: 3,
        platforms: "all",
        exe_candidates: &["dart.exe", "dart"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["pub", "global", "list"]),
            ("cacheDir", &["pub", "cache", "list"]),
            ("update", &["pub", "global", "activate", "{}"]),
            ("uninstall", &["pub", "global", "deactivate", "{}"]),
            ("install", &["pub", "global", "activate", "{}"]),
        ],
        cache_subdirs: &["hosted", "git"],
        cache_root_protected: true,
        download_url: "https://dart.dev/get-dart",
        docs_url: "https://dart.dev/tools/pub/cmd",
    },
    ManagerDef {
        id: "luarocks",
        name: "luarocks",
        language: "lua",
        tier: 3,
        platforms: "all",
        exe_candidates: &["luarocks.bat", "luarocks.exe", "luarocks"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--porcelain"]),
            ("configDir", &["config", "--lr-path"]),
            ("update", &["install", "{}"]),
            ("uninstall", &["remove", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["cache"],
        cache_root_protected: false,
        download_url: "https://luarocks.org/",
        docs_url: "https://github.com/luarocks/luarocks/wiki/luarocks",
    },
    ManagerDef {
        id: "cpan",
        name: "cpan",
        language: "perl",
        tier: 3,
        platforms: "all",
        exe_candidates: &["cpan.bat", "cpanm.bat", "cpan", "cpanm"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["-l"]),
            ("update", &["-i", "{}"]),
            ("uninstall", &["-U", "{}"]),
            ("install", &["-i", "{}"]),
        ],
        cache_subdirs: &["sources", "build"],
        cache_root_protected: false,
        download_url: "https://www.cpan.org/modules/INSTALL.html",
        docs_url: "https://metacpan.org/pod/CPAN",
    },
];

pub fn find(id: &str) -> Option<&'static ManagerDef> {
    MANAGERS.iter().find(|m| m.id == id)
}

/// 取出某个操作的白名单参数；返回 None 表示该操作不被允许。
pub fn op_args(id: &str, op: &str) -> Option<&'static [&'static str]> {
    find(id)?.ops.iter().find(|(name, _)| *name == op).map(|(_, args)| *args)
}

/// 允许被执行的**全部**操作名（用于错误提示、诊断面板与文档生成）
pub fn allowed_ops(id: &str) -> Vec<&'static str> {
    find(id).map(|m| m.ops.iter().map(|(n, _)| *n).collect()).unwrap_or_default()
}

/// 清理白名单：只有出现在这里的 (manager, kind) 组合才可能产生可删除候选。
/// 未列出的组合一律标记为 protected。
pub fn clean_allowed(manager: &str, kind: &str) -> bool {
    matches!(
        (manager, kind),
        ("npm", "cache")
            | ("npm", "temp")
            | ("pnpm", "cache")
            | ("yarn", "cache")
            | ("pip", "cache")
            | ("cargo", "cache")
            | ("winget", "cache")
            | ("composer", "cache")
            | ("gem", "cache")
            | ("go", "cache")
            | ("scoop", "cache")
            | ("conda", "cache")
            | ("dart", "cache")
            | ("luarocks", "cache")
    )
}

/// 一期管理器：界面上作为「可用」展示
pub fn tier1_ids() -> Vec<&'static str> {
    MANAGERS.iter().filter(|m| m.tier == 1).map(|m| m.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_manager_has_version_op() {
        for m in MANAGERS {
            assert!(op_args(m.id, "version").is_some(), "{} 缺少 version 操作", m.id);
        }
    }

    #[test]
    fn every_manager_has_download_url() {
        for m in MANAGERS {
            assert!(m.download_url.starts_with("https://"), "{} 的下载地址必须是 https", m.id);
        }
    }

    #[test]
    fn manager_ids_are_unique() {
        let mut ids: Vec<&str> = MANAGERS.iter().map(|m| m.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "manager id 存在重复");
    }

    #[test]
    fn tier1_covers_required_managers() {
        for required in ["pip", "npm", "pnpm", "yarn", "cargo", "dotnet", "winget"] {
            let def = find(required).unwrap_or_else(|| panic!("缺少一期管理器 {required}"));
            assert_eq!(def.tier, 1, "{required} 应属于一期");
        }
    }

    #[test]
    fn unknown_op_is_rejected() {
        assert!(op_args("npm", "execAnything").is_none());
        assert!(op_args("nonexistent", "version").is_none());
    }

    #[test]
    fn clean_whitelist_is_explicit() {
        assert!(clean_allowed("npm", "cache"));
        assert!(!clean_allowed("winget", "oldVersion"));
        assert!(!clean_allowed("maven", "cache"));
    }
}