//! 包管理器探测：定位可执行文件、版本、全局根目录、缓存目录、配置文件。

use crate::error::AppResult;
use crate::executor;
use crate::fsutil;
use crate::icons;
use crate::models::ManagerInfo;
use crate::packages;
use crate::registry;
use crate::validate;
use crate::whitelist;
use std::path::{Path, PathBuf};

/// 探测单个包管理器。任何子步骤失败都降级为 warning，不影响整体返回。
///
/// `theme` 决定 logo 的配色变体；切换主题后前端可调用 `manager_logos` 重新获取。
pub fn detect(
    id: &str,
    timeout_ms: u64,
    with_registry: bool,
    theme: icons::Theme,
) -> AppResult<ManagerInfo> {
    let def = whitelist::find(id).ok_or_else(|| crate::error::AppError::invalid(format!("不支持: {id}")))?;
    let mut warnings: Vec<String> = Vec::new();

    let exe = executor::resolve_executable(def.exe_candidates);
    if exe.is_none() {
        warnings.push(format!("未在 PATH 中找到 {}", def.exe_candidates.join(" / ")));
    }

    let version = match &exe {
        Some(path) => {
            let args = whitelist::op_args(id, "version").unwrap_or(&[]);
            let req = executor::ExecRequest::new(path.to_string_lossy().to_string(), args)
                .with_timeout_ms(timeout_ms.min(15_000));
            match executor::run_resolved(path, &req) {
                Ok(out) if out.success => out
                    .stdout
                    .lines()
                    .chain(out.stderr.lines())
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(|l| l.chars().take(90).collect()),
                Ok(out) => {
                    warnings.push(format!("版本命令失败: {}", out.failure_hint()));
                    None
                }
                Err(e) => {
                    warnings.push(format!("版本命令异常: {}", e.message));
                    None
                }
            }
        }
        None => None,
    };

    let global_root = global_root_for(id, timeout_ms);
    let cache_dir = cache_dir_for(id, timeout_ms);
    let config_file = registry::candidate_config_files(id, None)
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().to_string());

    // 镜像源只对「确实支持配置源」的管理器读取，避免无意义 IO
    let registry_config = if with_registry && exe.is_some() && supports_registry(id) {
        match registry::read(id, timeout_ms) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                warnings.push(format!("读取镜像源失败: {}", e.message));
                None
            }
        }
    } else {
        None
    };

    Ok(ManagerInfo {
        id: def.id.to_string(),
        name: def.name.to_string(),
        language: def.language.to_string(),
        tier: def.tier,
        detected: exe.is_some(),
        version,
        exe_path: exe.map(|p| p.to_string_lossy().to_string()),
        global_root: global_root.map(|p| p.to_string_lossy().to_string()),
        cache_dir: cache_dir.map(|p| p.to_string_lossy().to_string()),
        config_file,
        registry: registry_config,
        logo: Some(icons::manager_logo_svg(def.id, def.name, theme)),
        download_url: Some(def.download_url.to_string()),
        docs_url: Some(def.docs_url.to_string()),
        warnings,
    })
}

/// 该管理器的镜像源是否值得读取/编辑
pub fn supports_registry(id: &str) -> bool {
    matches!(
        id,
        "npm" | "pnpm" | "yarn" | "pip" | "cargo" | "go" | "composer" | "gem" | "conda" | "maven"
    )
}

/// 优先返回真实存在的候选；都不存在时返回第一个，供界面展示「预期路径」
///
/// 接受 `Option<PathBuf>` 序列，自动过滤 `None` —— 调用点常常混着
/// 「环境变量可能不存在」与「固定路径」两类候选。
fn prefer<I>(list: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = Option<PathBuf>>,
{
    let mut list: Vec<PathBuf> = list.into_iter().flatten().collect();
    if list.is_empty() {
        return None;
    }
    if let Some(found) = list.iter().find(|p| p.is_dir()) {
        return Some(found.clone());
    }
    Some(list.remove(0))
}

/// 全局包安装根目录
pub fn global_root_for(id: &str, timeout_ms: u64) -> Option<PathBuf> {
    // 1) 命令优先（最准确）
    if let Some(line) = packages::command_first_line(id, "rootGlobal", timeout_ms) {
        let p = PathBuf::from(line.trim().trim_matches('"'));
        if p.is_dir() {
            return Some(p);
        }
    }

    let home = validate::home_dir()?;
    let local = validate::env_dir("LOCALAPPDATA");
    let appdata = validate::env_dir("APPDATA");

    match id {
        "npm" => {
            let prefix = packages::command_first_line("npm", "prefixGlobal", timeout_ms)
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
                .or_else(|| appdata.clone());
            prefix.map(|p| p.join("node_modules")).filter(|p| p.is_dir())
        }
        "pnpm" => {
            let pnpm_home = validate::env_dir("PNPM_HOME").or_else(|| local.map(|l| l.join("pnpm")));
            let global_dir = pnpm_home?.join("global");
            let mut versions: Vec<PathBuf> = std::fs::read_dir(&global_dir)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            versions.last().map(|v| v.join("node_modules")).filter(|p| p.is_dir())
        }
        "yarn" => {
            let base = local.or(appdata)?;
            let p = base.join("Yarn").join("Data").join("global").join("node_modules");
            if p.is_dir() {
                Some(p)
            } else {
                prefer([Some(home.join(".config").join("yarn").join("global").join("node_modules"))])
            }
        }
        "cargo" => prefer([Some(home.join(".cargo").join("bin"))]),
        // NuGet：<home>/.nuget/packages/<PackageId>/<Version>
        "dotnet" => prefer([Some(home.join(".nuget").join("packages"))]),
        // PowerShellGet：<Documents>/PowerShell/Modules
        "powershellget" => prefer([
            Some(home.join("Documents").join("PowerShell").join("Modules")),
            Some(home.join("Documents").join("WindowsPowerShell").join("Modules")),
        ]),
        "composer" => prefer([
            appdata.as_ref().map(|a| a.join("Composer")),
            Some(home.join(".composer")),
        ]),
        "gem" => prefer([Some(home.join(".gem"))]),
        "go" => packages::command_first_line("go", "cacheDir", timeout_ms)
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(|| prefer([Some(home.join("go").join("pkg").join("mod"))])),
        "maven" => prefer([Some(home.join(".m2").join("repository"))]),
        "chocolatey" => prefer([
            validate::env_dir("ChocolateyInstall"),
            Some(PathBuf::from("C:\\ProgramData\\chocolatey")),
        ]),
        "scoop" => prefer([
            validate::env_dir("SCOOP")
                .or_else(|| Some(home.join("scoop")))
                .map(|s| s.join("apps")),
        ]),
        "conda" => prefer([
            validate::env_dir("CONDA_PREFIX"),
            Some(home.join("miniconda3")),
            Some(home.join("anaconda3")),
        ]),
        // pub 全局包：<PUB_CACHE>/global_packages（PUB_CACHE 默认 %LOCALAPPDATA%\Pub\Cache）
        "dart" => prefer([validate::env_dir("PUB_CACHE")
            .or_else(|| local.map(|l| l.join("Pub").join("Cache")))
            .map(|c| c.join("global_packages"))]),
        "luarocks" => prefer([
            Some(home.join("AppData").join("luarocks")),
            Some(home.join(".luarocks")),
        ]),
        "cpan" => prefer([Some(home.join(".cpan"))]),
        _ => None,
    }
}

/// 缓存目录
pub fn cache_dir_for(id: &str, timeout_ms: u64) -> Option<PathBuf> {
    // 先问命令（npm/pnpm/yarn/composer/gem/choco/conda/uv 等都支持）
    if let Some(line) = packages::command_first_line(id, "cacheDir", timeout_ms) {
        let raw = line.trim().trim_matches('"');
        if !raw.is_empty() && raw != "undefined" && raw != "null" {
            let p = validate::expand_tilde(raw);
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    let home = validate::home_dir()?;
    let local = validate::env_dir("LOCALAPPDATA");
    let appdata = validate::env_dir("APPDATA");
    let nodir = |p: PathBuf| -> Vec<Option<PathBuf>> { vec![Some(p)] };

    match id {
        "npm" => prefer([
            local.as_ref().map(|l| l.join("npm-cache")),   // npm v7+
            appdata.as_ref().map(|a| a.join("npm-cache")), // npm v6 及更早
        ]),
        "pnpm" => prefer([
            local.as_ref().map(|l| l.join("pnpm").join("store")),
            local.as_ref().map(|l| l.join("pnpm-store")),
            Some(home.join(".pnpm-store")),
        ]),
        "yarn" => prefer(nodir(
            local.clone().unwrap_or_else(|| home.clone()).join("Yarn").join("Cache"),
        )),
        "pip" => prefer(nodir(
            local.clone().unwrap_or_else(|| home.join(".cache")).join("pip").join("Cache"),
        )),
        "cargo" => prefer(nodir(home.join(".cargo").join("registry"))),
        "dotnet" => prefer([
            local.as_ref().map(|l| l.join("NuGet").join("v3-cache")),
            Some(home.join(".local").join("share").join("NuGet").join("v3-cache")),
        ]),
        "powershellget" => prefer(nodir(
            home.join(".local").join("share").join("powershell").join("ModuleAnalysisCache"),
        )),
        "composer" => prefer([
            local.as_ref().map(|l| l.join("Composer").join("cache")),
            appdata.as_ref().map(|a| a.join("Composer").join("cache")),
        ]),
        "gem" => prefer(nodir(home.join(".gem"))),
        "go" => prefer(nodir(home.join("go").join("pkg").join("mod"))),
        "maven" => prefer(nodir(home.join(".m2").join("repository"))),
        "winget" => prefer(nodir(
            local.clone().unwrap_or_else(|| home.join(".cache")).join("Microsoft").join("WinGet"),
        )),
        "chocolatey" => prefer([
            validate::env_dir("ChocolateyInstall"),
            Some(PathBuf::from("C:\\ProgramData\\chocolatey")),
        ]),
        "scoop" => prefer([validate::env_dir("SCOOP")
            .or_else(|| Some(home.join("scoop")))
            .map(|s| s.join("cache"))]),
        "conda" => prefer(nodir(home.join(".conda").join("pkgs"))),
        "dart" => prefer(nodir(
            validate::env_dir("PUB_CACHE")
                .or_else(|| local.map(|l| l.join("Pub").join("Cache")))
                .unwrap_or_else(|| home.join(".pub-cache")),
        )),
        "luarocks" => prefer(nodir(home.join("AppData").join("luarocks").join("cache"))),
        "cpan" => prefer(nodir(home.join(".cpan").join("sources"))),
        _ => None,
    }
}

/// 判断两个路径是否指向同一位置
pub fn same_dir(a: &Path, b: &Path) -> bool {
    validate::normalize(a) == validate::normalize(b)
}

/// 缓存统计（供 scan 与 cache 命令共用）
pub fn cache_stats(id: &str, timeout_ms: u64) -> AppResult<crate::models::CacheStats> {
    let path = cache_dir_for(id, timeout_ms)
        .ok_or_else(|| crate::error::AppError::not_installed(&format!("{id} 的缓存目录")))?;
    let stat = fsutil::dir_stat(&path, fsutil::MAX_SCAN_ENTRIES);
    let children = fsutil::children_stat(&path, 100_000)
        .into_iter()
        .take(60)
        .map(|(name, child_path, cs)| crate::models::CacheChild {
            name,
            path: child_path.to_string_lossy().to_string(),
            bytes: cs.bytes,
            file_count: cs.file_count,
        })
        .collect();

    Ok(crate::models::CacheStats {
        manager_id: id.to_string(),
        path: path.to_string_lossy().to_string(),
        exists: path.is_dir(),
        total_bytes: stat.bytes,
        file_count: stat.file_count,
        last_modified: stat.last_modified.map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.to_rfc3339()
        }),
        children,
        truncated: stat.truncated,
    })
}

/// 每个管理器的推荐安装方式（未安装时提示用户）
pub fn install_hint_for(id: &str) -> Option<String> {
    let hint = match id {
        "npm" | "pnpm" | "yarn" => "安装 Node.js 后可用 npm 安装：npm i -g pnpm",
        "pip" => "随 Python 一起安装，或用 get-pip.py 单独安装",
        "cargo" => "通过 rustup 安装：https://rustup.rs/",
        "dotnet" => "安装 .NET SDK：winget install Microsoft.DotNet.SDK.8",
        "winget" => "随「应用安装程序」提供，可从 Microsoft Store 更新",
        "powershellget" => "安装 PowerShell 7：winget install Microsoft.PowerShell",
        "composer" => "下载 composer-setup.php 后运行安装程序",
        "gem" => "随 Ruby 一起安装（RubyInstaller for Windows）",
        "go" => "下载官方安装包：https://go.dev/dl/",
        "maven" => "解压后把 bin 目录加入 PATH（需先安装 JDK）",
        "chocolatey" => "以管理员身份运行安装脚本：https://chocolatey.org/install",
        "scoop" => "PowerShell 中运行：irm get.scoop.sh | iex",
        "conda" => "安装 Miniconda：https://docs.conda.io/en/latest/miniconda.html",
        "dart" => "随 Flutter SDK 提供，或单独安装 Dart SDK",
        "luarocks" => "下载 Windows 版安装包：https://luarocks.org/",
        "cpan" => "随 Perl 一起安装（Strawberry Perl 已内置）",
        _ => return None,
    };
    Some(hint.to_string())
}
