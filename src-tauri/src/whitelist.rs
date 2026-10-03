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

    // ======================= 四期：语言生态与运行时 =======================
    // 说明：这一批里有些生态**没有「全局已安装包」这个概念**（deno、mix 的依赖都是
    // 项目级的），因此下面刻意不写 listGlobal —— 界面上会说明原因，
    // 而不是显示成「0 个包」让用户以为工具坏了。
    ManagerDef {
        id: "bun",
        name: "bun",
        language: "node",
        tier: 4,
        platforms: "all",
        exe_candidates: &["bun.exe", "bun.cmd", "bun"],
        ops: &[
            ("version", &["--version"]),
            // 注意：`bun pm ls -g` 在官方文档里没有写，是从上游源码读出来的
            // （PackageManager::init 在 global 标记下会 chdir 到全局目录）。
            // 若某个 bun 版本不支持，扫描会退化为「读不到包」而不是报错崩溃。
            ("listGlobal", &["pm", "ls", "-g"]),
            ("rootGlobal", &["pm", "ls", "-g"]),
            ("update", &["update", "-g", "{}"]),
            // --global 只在文档的 usage 块出现、正文无示例，标为需验证
            ("uninstall", &["remove", "-g", "{}"]),
            ("install", &["add", "-g", "{}"]),
        ],
        cache_subdirs: &["install", "cache"],
        cache_root_protected: false,
        download_url: "https://bun.sh/docs/installation",
        docs_url: "https://bun.sh/docs/pm/cli/pm",
    },
    ManagerDef {
        id: "deno",
        name: "deno",
        language: "typescript",
        tier: 4,
        platforms: "all",
        exe_candidates: &["deno.exe", "deno.cmd", "deno"],
        ops: &[
            ("version", &["--version"]),
            // 刻意不提供 listGlobal：deno **没有**列出全局已安装包的命令。
            // `deno info` 只打印缓存路径；`deno list` 是 2026-06 才加的、且只列项目依赖。
            // 全局安装只是往 bin 目录写 shim，因此改由 packages.rs 直接枚举目录。
            ("info", &["info"]),
            // 全局安装要显式 -g；1.x 时代 install 默认就是全局，语义相反，
            // 因此这里始终显式带上 -g 以免行为随版本漂移。
            ("install", &["install", "-g", "{}"]),
            ("uninstall", &["uninstall", "-g", "{}"]),
            // 没有「更新已安装的全局脚本」命令，只能重装（界面会说明）
        ],
        cache_subdirs: &["remote", "npm", "registries", "gen"],
        cache_root_protected: false,
        download_url: "https://docs.deno.com/runtime/getting_started/installation/",
        docs_url: "https://docs.deno.com/runtime/reference/cli/",
    },
    ManagerDef {
        id: "julia",
        name: "Julia Pkg",
        language: "julia",
        tier: 4,
        platforms: "all",
        exe_candidates: &["julia.exe", "julia"],
        ops: &[
            ("version", &["--version"]),
            // 列的是**活动环境**里显式添加的包（PKGMODE_PROJECT），不是"全局"。
            // 界面需要说明这一点，否则用户会以为看到的是整机所有 Julia 包。
            ("listGlobal", &["-e", "using Pkg; Pkg.status()"]),
            // Pkg.update("pkg") 只更新指定包；不带参数的 Pkg.update() 会更新**全部**，
            // 那与右键菜单「更新此包」的语义不符
            ("update", &["-e", "using Pkg; Pkg.update(\"{}\")"]),
            // Pkg.rm 只改 Project.toml，**不删除文件**；真正的清理是仓库级的 Pkg.gc()，
            // 绝不能挂到「卸载这个包」上（那会清掉所有项目共享的内容）。
            ("uninstall", &["-e", "using Pkg; Pkg.rm(\"{}\")"]),
            ("install", &["-e", "using Pkg; Pkg.add(\"{}\")"]),
        ],
        cache_subdirs: &["packages", "artifacts", "compiled", "logs", "scratchspaces"],
        cache_root_protected: false,
        download_url: "https://julialang.org/downloads/",
        docs_url: "https://pkgdocs.julialang.org/v1/",
    },
    ManagerDef {
        id: "mix",
        name: "Hex / mix",
        language: "elixir",
        tier: 4,
        platforms: "all",
        exe_candidates: &["mix.bat", "mix"],
        ops: &[
            // 探测目标是 mix 而不是 hex：Hex 本身不是可执行文件，
            // 它是 `mix local.hex` 装进 ~/.mix/archives 的 archive，只能以 `mix hex.*` 调用。
            ("version", &["hex.info"]),
            // 刻意不提供 listGlobal：Elixir 的依赖是**项目级**的（mix deps），
            // 没有全局已安装列表。
            // 刻意不提供 install：装包需要先编辑 mix.exs 再 mix deps.get，
            // 不是一条命令能完成的；全局 escript 安装属于另一回事且会执行代码。
            // 卸载仅对全局 escript 有意义，因此明确标注作用域。
            ("uninstall", &["escript.uninstall", "{}"]),
        ],
        cache_subdirs: &["archives", "escripts"],
        cache_root_protected: false,
        download_url: "https://elixir-lang.org/install/",
        docs_url: "https://hexdocs.pm/hex/",
    },

    // ======================= 四期：构建工具与系统级 =======================
    ManagerDef {
        id: "gradle",
        name: "Gradle",
        language: "java",
        tier: 4,
        platforms: "all",
        exe_candidates: &["gradle.bat", "gradle"],
        ops: &[
            ("version", &["--version"]),
            // 刻意**只保留 version**。理由不是"没查到命令"，而是安全：
            // Gradle 的每一次调用都会执行项目的构建脚本 —— 也就是说
            // `gradle <task>` 本质上是任意代码执行，静态参数白名单约束不了它。
            // 而且 Gradle 没有全局包仓库，dependencies 之类的都是项目级任务，
            // 在非项目目录下必然失败。
        ],
        cache_subdirs: &["caches", "wrapper", "daemon", "native"],
        cache_root_protected: false,
        download_url: "https://gradle.org/install/",
        docs_url: "https://docs.gradle.org/current/userguide/userguide.html",
    },
    ManagerDef {
        id: "brew",
        name: "Homebrew",
        language: "system",
        tier: 4,
        // 无 Windows 原生版本（只能在 WSL 里用）
        platforms: "unix",
        exe_candidates: &["brew"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--versions"]),
            ("leaves", &["leaves"]),
            ("update", &["upgrade", "{}"]),
            // 刻意不用 --zap/--force/--ignore-dependencies：
            // --zap 会删掉 cask 关联的**所有**文件（含应用间共享的），
            // --force 会删掉该 formula 的全部已安装版本，
            // --ignore-dependencies 会移除别的包正依赖的东西。
            ("uninstall", &["uninstall", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["downloads"],
        cache_root_protected: false,
        download_url: "https://brew.sh/",
        docs_url: "https://docs.brew.sh/Manpage",
    },
    ManagerDef {
        id: "vcpkg",
        name: "vcpkg",
        language: "cpp",
        tier: 4,
        platforms: "all",
        exe_candidates: &["vcpkg.exe", "vcpkg"],
        ops: &[
            ("version", &["version"]),
            // 注意语义：classic 模式下这是**该 vcpkg 实例全局共享**的已安装树，
            // 被所有使用它的项目共享；manifest 模式下才是项目级（vcpkg_installed/）。
            ("listGlobal", &["list"]),
            ("update", &["upgrade", "--no-dry-run", "{}"]),
            // 刻意不加 --recurse：那会允许移除**命令行未点名**的包。
            // 注意 vcpkg 的 remove 会级联移除依赖它的包。
            ("uninstall", &["remove", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["downloads", "buildtrees", "packages"],
        cache_root_protected: false,
        download_url: "https://learn.microsoft.com/en-us/vcpkg/get_started/get-started",
        docs_url: "https://learn.microsoft.com/en-us/vcpkg/",
    },
    ManagerDef {
        id: "conan",
        name: "Conan",
        language: "cpp",
        tier: 4,
        platforms: "all",
        exe_candidates: &["conan.exe", "conan"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "*", "--format=json"]),
            // 用 install --update=<pkg> 而不是裸 update：避免全量重解析
            ("update", &["install", "--update={}", "."]),
            // 【重要】这里的 `{}` 会被替换成包名，而 Conan 的模式语法意味着
            // `*` 能清空整个本地缓存。防线是 validate::system_package_name ——
            // 它会拒绝 `*` 等一切通配字符，因此 `{}` 不可能是模式表达式。
            ("uninstall", &["remove", "{}"]),
            // Conan 2 的安装是「按需求安装」而非「按名安装」：--requires 需要一个
            // 带版本或范围的引用。这里用裸包名，由 conan 自行解析最新版本。
            ("install", &["install", "--requires={}"]),
        ],
        cache_subdirs: &["p", "b"],
        cache_root_protected: false,
        download_url: "https://docs.conan.io/2/installation.html",
        docs_url: "https://docs.conan.io/2/",
    },
    ManagerDef {
        id: "swift",
        name: "Swift Package Manager",
        language: "swift",
        tier: 4,
        platforms: "all",
        exe_candidates: &["swift.exe", "swift"],
        ops: &[
            ("version", &["--version"]),
            // 刻意不提供 listGlobal/install/uninstall：
            // SwiftPM 的依赖是**项目级**的（改 Package.swift 后 package resolve），
            // 全局只有缓存可清，没有"已安装包"这个概念。
            // 唯一带"安装"语义的是把当前项目的可执行产物装到 ~/.swiftpm/bin，
            // 那不是依赖管理，替用户执行没有意义。
            // `package update` 需要显式给出包名，否则会更新全部依赖。
            ("update", &["package", "update", "{}"]),
        ],
        cache_subdirs: &["cache", "security"],
        cache_root_protected: false,
        download_url: "https://www.swift.org/install/",
        docs_url: "https://docs.swift.org/swiftpm/documentation/packagemanagerdocs/",
    },
    ManagerDef {
        id: "cocoapods",
        name: "CocoaPods",
        language: "ruby",
        tier: 4,
        // 需要 Xcode，仅 macOS
        platforms: "macos",
        exe_candidates: &["pod.bat", "pod"],
        ops: &[
            ("version", &["--version"]),
            // 刻意只保留 version。CocoaPods 没有"已安装包列表"：
            //   `pod list` 列的是**可用**的 pod 目录，不是已安装的；
            //   依赖是项目级的（Podfile.lock）。
            // 而 pod install/update 会改写你的 .xcodeproj / .xcworkspace，
            // 且 Podfile 是按 Ruby 求值的 —— 即安装过程就是任意代码执行。
            // 因此不替用户执行这些动作。
            ("outdated", &["outdated"]),
        ],
        cache_subdirs: &["CocoaPods"],
        cache_root_protected: false,
        download_url: "https://guides.cocoapods.org/using/getting-started.html",
        docs_url: "https://guides.cocoapods.org/terminal/commands.html",
    },

    // ======================= 五期：Linux 发行版包管理器 =======================
    // 这些**全部仅 Linux**，且在 Windows 上永远检测不到（界面会标注平台不适用）。
    // 它们的写操作几乎都需要 root：工具本身不提权，命令会以非 root 失败并把
    // 错误如实回显，这是刻意选择 —— 不静默 sudo。
    ManagerDef {
        id: "apt",
        name: "apt",
        language: "system",
        tier: 5,
        platforms: "linux",
        // 用 apt-get 而不是 apt：apt(8) 自己建议在脚本中使用专用工具
        exe_candidates: &["apt-get"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--installed"]),
            ("update", &["install", "-y", "--only-upgrade", "{}"]),
            // 刻意不用 autoremove/dist-upgrade：前者会移除"不再被需要"的自动依赖
            //（apt 自己的 man 页都提醒要先检查列表），后者可能移除已安装的包。
            ("uninstall", &["remove", "-y", "{}"]),
            ("install", &["install", "-y", "{}"]),
        ],
        cache_subdirs: &["archives"],
        cache_root_protected: false,
        download_url: "https://www.debian.org/distrib/",
        docs_url: "https://manpages.debian.org/bookworm/apt/apt-get.8.en.html",
    },
    ManagerDef {
        id: "pacman",
        name: "pacman",
        language: "system",
        tier: 5,
        platforms: "linux",
        exe_candidates: &["pacman"],
        ops: &[
            ("version", &["--version"]),
            // -Qq：每行一个包名。不用 -Q 是因为那会带上版本并需要切分。
            ("listGlobal", &["-Qq"]),
            // -Qu 只列出可升级的（只读）
            ("outdated", &["-Qu"]),
            // 刻意不用 -Rdd（跳过依赖检查，官方 wiki 警告可能移除关键依赖导致系统损坏）、
            // -Rc（级联移除所有依赖它的包）、-Sy 单独使用（部分升级，官方明令禁止）。
            ("uninstall", &["-R", "--noconfirm", "{}"]),
            ("install", &["-S", "--needed", "--noconfirm", "{}"]),
        ],
        cache_subdirs: &["pkg"],
        cache_root_protected: false,
        download_url: "https://archlinux.org/download/",
        docs_url: "https://man.archlinux.org/man/pacman.8.en",
    },
    ManagerDef {
        id: "dnf",
        name: "dnf / yum",
        language: "system",
        tier: 5,
        platforms: "linux",
        // yum 在 RHEL 8/9 上只是指向 dnf 的兼容层，因此合并为一个条目
        exe_candidates: &["dnf5", "dnf", "yum"],
        ops: &[
            ("version", &["--version"]),
            // --disableexcludes=all：否则 excludepkgs 会让已安装的包从列表里消失
            ("listGlobal", &["list", "--installed", "--disableexcludes=all"]),
            ("update", &["upgrade", "-y", "{}"]),
            // --noautoremove：默认会连带移除"因此变得不再需要"的依赖，
            // 这超出了"卸载这个包"的语义
            ("uninstall", &["remove", "-y", "--noautoremove", "{}"]),
            ("install", &["install", "-y", "{}"]),
        ],
        cache_subdirs: &["packages", "metadata"],
        cache_root_protected: false,
        download_url: "https://docs.fedoraproject.org/en-US/quick-docs/dnf/",
        docs_url: "https://dnf.readthedocs.io/en/latest/command_ref.html",
    },
    ManagerDef {
        id: "flatpak",
        name: "Flatpak",
        language: "system",
        tier: 5,
        platforms: "linux",
        exe_candidates: &["flatpak"],
        ops: &[
            ("version", &["--version"]),
            // 必须显式指定列：man 页没有规定默认列，解析不能依赖默认输出
            (
                "listGlobal",
                &["list", "--columns=application,version,branch,installation,origin"],
            ),
            // 只更新指定的 ref，避免影响全部应用
            ("update", &["update", "--noninteractive", "{}"]),
            // 刻意不用 --unused / --all / --delete-data：
            // --unused 会移除任何"当前未被需要"的运行时，--delete-data 是不可逆的数据删除
            ("uninstall", &["uninstall", "--noninteractive", "{}"]),
            // 安装需要显式给出远程仓库名，否则会交互式提问
            ("install", &["install", "--noninteractive", "flathub", "{}"]),
        ],
        cache_subdirs: &["app", "runtime", "repo"],
        cache_root_protected: false,
        download_url: "https://flatpak.org/setup/",
        docs_url: "https://docs.flatpak.org/en/latest/flatpak-command-reference.html",
    },
    ManagerDef {
        id: "snap",
        name: "Snap",
        language: "system",
        tier: 5,
        platforms: "linux",
        exe_candidates: &["snap"],
        // 注意：snap 的版本子命令是 `snap version`，没有 `--version` 全局标志
        ops: &[
            ("version", &["version"]),
            ("listGlobal", &["list"]),
            ("update", &["refresh", "{}"]),
            // 刻意不用 --purge：正常移除会先做数据快照（保留约 31 天），
            // --purge 会跳过快照，使移除不可恢复
            ("uninstall", &["remove", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["cache"],
        cache_root_protected: false,
        download_url: "https://snapcraft.io/docs/installing-snapd",
        docs_url: "https://snapcraft.io/docs",
    },
    ManagerDef {
        id: "pipx",
        name: "pipx",
        language: "python",
        tier: 5,
        // 唯一跨平台的：Linux / macOS / Windows
        platforms: "all",
        exe_candidates: &["pipx.exe", "pipx"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["list", "--output", "json"]),
            // 只升级指定包；不用 upgrade-all（那是"全部"语义）
            ("update", &["upgrade", "{}"]),
            ("uninstall", &["uninstall", "{}"]),
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["venvs", "shared"],
        cache_root_protected: false,
        download_url: "https://pipx.pypa.io/latest/how-to/install-pipx.html",
        docs_url: "https://pipx.pypa.io/latest/",
    },

    // ======================= 六期：语言工具链 =======================
    ManagerDef {
        id: "opam",
        name: "opam",
        language: "ocaml",
        tier: 6,
        platforms: "all",
        exe_candidates: &["opam.exe", "opam"],
        ops: &[
            ("version", &["--version"]),
            // -s/--short：每行一个包名、不带表头。
            // 注意这是**当前 switch** 的包，不是跨 switch 的全局列表
            //（opam 本来就没有跨 switch 列表）。
            ("listGlobal", &["list", "--installed", "--short"]),
            // 两步：先刷新索引再升级
            ("updateIndex", &["update"]),
            ("update", &["upgrade", "{}", "-y"]),
            ("uninstall", &["remove", "{}", "-y"]),
            ("install", &["install", "{}", "-y"]),
        ],
        cache_subdirs: &["download-cache", "repo"],
        cache_root_protected: false,
        download_url: "https://opam.ocaml.org/doc/Install.html",
        docs_url: "https://opam.ocaml.org/doc/",
    },
    ManagerDef {
        id: "dub",
        name: "dub",
        language: "dlang",
        tier: 6,
        platforms: "all",
        exe_candidates: &["dub.exe", "dub"],
        ops: &[
            // --version 只在**第一个参数**位置才被识别，因此必须是 argv[1]
            ("version", &["--version"]),
            // dub list 列的是「缓存 + 搜索路径 + 手动注册」的包，不是项目依赖。
            // --color=never 明确禁用着色（非 TTY 时本来也会自动关闭）
            ("listGlobal", &["list", "--color=never"]),
            // 刻意不提供 update：`dub upgrade` 升级的是**项目的**依赖
            //（会改写 dub.selections.json），不接受包名，因此不是「更新此包」。
            // 索引刷新由 dub 自行处理，也没有独立的 refresh 子命令。
            // 只删缓存中的一个版本。刻意不加 --version=*（会删掉**全部**缓存版本，
            // 而缓存是用户级共享的，会连带影响其它项目）
            ("uninstall", &["remove", "{}", "-n", "--color=never"]),
            // 刻意不提供 install：dub 没有 install 命令。
            // `dub add` 是"加项目依赖"（会改写 dub.json/dub.sdl），
            // 而 dub **没有任何命令能移除项目依赖** —— 那需要手工改文件。
            // 把 add 当 install 会让用户以为装了全局包，语义不符，因此留空。
        ],
        cache_subdirs: &["packages", "cache"],
        cache_root_protected: false,
        download_url: "https://dub.pm/getting-started/install/",
        docs_url: "https://dub.pm/cli-reference/dub/",
    },
    ManagerDef {
        id: "nimble",
        name: "nimble",
        language: "nim",
        tier: 6,
        platforms: "all",
        exe_candidates: &["nimble.exe", "nimble"],
        ops: &[
            ("version", &["--version"]),
            // --ver 才会输出版本（且是树形）；--nocolor 去掉 ANSI 着色。
            // 注意不带 --ver 时**完全没有版本号**，只有包名
            ("listGlobal", &["list", "--installed", "--ver", "--nocolor"]),
            ("updateIndex", &["refresh"]),
            // 刻意不提供 update：`nimble upgrade` 已被官方标记为废弃，
            // 替代的 `nimble lock --refresh` 是锁文件语义，不是"升级这个包"
            ("uninstall", &["uninstall", "{}", "-y", "--nocolor"]),
            // nimble install 默认就是全局（~/.nimble）
            ("install", &["install", "{}", "-y", "--nocolor"]),
        ],
        cache_subdirs: &["pkgs2", "buildtemp"],
        cache_root_protected: false,
        download_url: "https://nim-lang.org/install_windows.html",
        docs_url: "https://github.com/nim-lang/nimble",
    },
    ManagerDef {
        id: "cabal",
        name: "cabal",
        language: "haskell",
        tier: 6,
        platforms: "all",
        exe_candidates: &["cabal.exe", "cabal"],
        ops: &[
            // --numeric-version 输出纯版本号，便于直接展示
            ("version", &["--numeric-version"]),
            // --simple-output：每行 `name version`。
            // 注意它读的是 **GHC 包数据库**，不是 cabal store
            ("listGlobal", &["list", "--installed", "--simple-output"]),
            ("updateIndex", &["update"]),
            ("update", &["install", "{}"]),
            // 刻意不提供 uninstall：cabal **没有** uninstall 命令，
            // store 是内容寻址且跨项目共享的，删它会影响所有项目。
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["store", "packages"],
        cache_root_protected: false,
        download_url: "https://www.haskell.org/ghcup/",
        docs_url: "https://cabal.readthedocs.io/en/stable/cabal-commands.html",
    },
    ManagerDef {
        id: "stack",
        name: "stack",
        language: "haskell",
        tier: 6,
        platforms: "all",
        exe_candidates: &["stack.exe", "stack"],
        ops: &[
            ("version", &["--numeric-version"]),
            // 没有全局已安装列表。`stack ls globals` 列的是 **GHC 自带**的包
            //（不是 snapshot 包），但它至少是真实存在的"本机相关包"信息
            ("listGlobal", &["ls", "globals"]),
            ("updateIndex", &["update"]),
            ("update", &["install", "{}"]),
            // 刻意不提供 uninstall：`stack uninstall` 是个**空操作**，只打印建议。
            // 真正卸载要手工删 `stack path --local-bin` 下的文件，
            // 那不是"包管理"语义，不适合做成按钮。
            ("install", &["install", "{}"]),
        ],
        cache_subdirs: &["pantry", "indices", "snapshots", "programs"],
        cache_root_protected: false,
        download_url: "https://www.haskell.org/ghcup/",
        docs_url: "https://docs.haskellstack.org/en/stable/commands/",
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

/// 该管理器是否把包名当作**规格表达式**（而非纯字符串）处理。
///
/// 【为什么需要这个区分】
/// 系统级包管理器会对包名做模式匹配：
/// - `apt-get`：不匹配的参数只要含 `.` `?` `*` 就被当 POSIX 正则，按子串匹配全部包名
///   （man 页的例子是 `lo.*` 命中 `how-lo` 与 `lowest`）；
/// - `dnf remove`：包规格支持 `*` `?` `[]`，且 dnf 会自己展开；
/// - `pacman`：包名可带 `group/` 之类前缀。
///
/// 这些生态的合法包名只用到 `[a-z0-9+._-]`，不需要 `@` 与 `/`，
/// 因此 `validate::resolve_package_op` 会对它们改用更严格的字符集校验。
pub fn uses_spec_syntax(id: &str) -> bool {
    matches!(id, "apt" | "dnf" | "pacman" | "flatpak" | "snap" | "brew")
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

    /// 每个管理器的 platforms 取值必须合法，且必须能判断是否适用于本机
    #[test]
    fn platforms_are_declared_and_evaluated() {
        for m in MANAGERS {
            assert!(
                matches!(m.platforms, "all" | "win" | "macos" | "linux" | "unix"),
                "{} 的 platforms 取值非法: {}",
                m.id,
                m.platforms
            );
            assert!(!platform_label(m.platforms).is_empty());
            // 不应 panic；结果取决于当前系统
            let _ = platform_applies(m.platforms);
        }
    }

    /// 仅某平台的工具在本机必须被判为「不适用」——
    /// 否则 Windows 上会显示「未在 PATH 中找到 apt」，让用户以为是自己环境有问题
    #[test]
    fn os_specific_managers_are_gated_on_this_platform() {
        let linux_only = ["apt", "pacman", "dnf", "flatpak", "snap"];
        let macos_only = ["cocoapods"];
        let win_only = ["winget", "chocolatey", "scoop"];

        if CURRENT_OS == "win" {
            for id in linux_only.iter().chain(macos_only.iter()) {
                assert!(!applies_here(id), "{id} 不应在 Windows 上被判为适用");
            }
            for id in win_only {
                assert!(applies_here(id), "{id} 应在 Windows 上适用");
            }
        } else {
            // 其它平台上也应有一致的判断（至少不能 panic 且 win-only 不适用）
            for id in win_only {
                assert!(!applies_here(id), "{id} 不应在非 Windows 上被判为适用");
            }
        }
        // 跨平台的必须始终适用
        for id in ["npm", "pip", "cargo", "pipx", "opam", "cabal", "vcpkg", "conan"] {
            assert!(applies_here(id), "{id} 应跨平台适用");
        }
    }

    /// 把包名当**规格表达式**处理的生态必须是少数派，且只包含确实如此的
    #[test]
    fn spec_syntax_managers_are_explicitly_listed() {
        for id in ["apt", "dnf", "pacman"] {
            assert!(uses_spec_syntax(id), "{id} 会做正则/glob 展开，应走严格校验");
        }
        // 这些生态的包名不需要正则语义，不该被误伤
        for id in ["npm", "pip", "cargo", "bun", "vcpkg", "conan", "opam"] {
            assert!(!uses_spec_syntax(id), "{id} 不该被当成规格语法");
        }
    }

    /// 新增管理器的 id 必须唯一（否则 find 会取到错的那个），且数量要对得上
    #[test]
    fn all_managers_have_unique_ids_and_expected_count() {
        let mut ids: Vec<&str> = MANAGERS.iter().map(|m| m.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "manager id 存在重复");
        assert!(total >= 39, "管理器数量异常: {total}");
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