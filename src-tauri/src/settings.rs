//! 设置持久化与受控的外部链接打开。
//!
//! 设置存放于 `%APPDATA%/PackInspect/settings.json`（macOS/Linux 为对应的配置目录），
//! 采用「读取失败即回退默认值」的宽松策略：配置文件损坏不应让应用起不来。

use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::models::AppSettings;
use crate::validate;
use std::path::PathBuf;

/// 设置文件路径
pub fn settings_path() -> AppResult<PathBuf> {
    // 优先环境变量：中文用户名下 dirs 的已知文件夹查询可能失败
    let base = validate::env_dir("APPDATA")
        .map(|p| p.join("PackInspect"))
        .or_else(|| {
            validate::home_dir().map(|h| h.join(".config").join("PackInspect"))
        })
        .ok_or_else(|| AppError::io("无法定位用户配置目录"))?;
    Ok(base.join("settings.json"))
}

/// 读取设置；任何异常都回退到默认值
pub fn load() -> AppSettings {
    let Ok(path) = settings_path() else {
        return AppSettings::default();
    };
    if !path.is_file() {
        return AppSettings::default();
    }
    match fsutil::read_json(&path, 256 * 1024) {
        Ok(Some(value)) => AppSettings {
            language: value
                .get("language")
                .and_then(|v| v.as_str())
                .filter(|s| matches!(*s, "zh-CN" | "en-US"))
                .unwrap_or("zh-CN")
                .to_string(),
            theme: value
                .get("theme")
                .and_then(|v| v.as_str())
                .filter(|s| matches!(*s, "dark" | "light" | "system"))
                .unwrap_or("dark")
                .to_string(),
            scan_on_startup: value.get("scanOnStartup").and_then(|v| v.as_bool()).unwrap_or(true),
            show_other_platforms: value
                .get("showOtherPlatforms")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        },
        _ => AppSettings::default(),
    }
}

/// 写入设置
pub fn save(settings: &AppSettings) -> AppResult<String> {
    // 再校验一次：前端可能传来任意值
    if !matches!(settings.language.as_str(), "zh-CN" | "en-US") {
        return Err(AppError::invalid(format!("不支持的语言: {}", settings.language)));
    }
    if !matches!(settings.theme.as_str(), "dark" | "light" | "system") {
        return Err(AppError::invalid(format!("不支持的主题: {}", settings.theme)));
    }
    let path = settings_path()?;
    let payload = serde_json::json!({
        "language": settings.language,
        "theme": settings.theme,
        "scanOnStartup": settings.scan_on_startup,
        "showOtherPlatforms": settings.show_other_platforms,
    });
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|e| AppError::internal(format!("序列化设置失败: {e}")))?;
    fsutil::write_text(&path, &text)?;
    Ok(path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// 打开外部链接
// ---------------------------------------------------------------------------

/// 允许打开的外部域名白名单（包管理器官网 + 各大包仓库）。
/// 采用**后缀匹配**，例如 `www.npmjs.com` 与 `npmjs.com` 都命中 `npmjs.com`。
const ALLOWED_HOSTS: &[&str] = &[
    // 包管理器官网
    "nodejs.org",
    "pnpm.io",
    "yarnpkg.com",
    "bootstrap.pypa.io",
    "pip.pypa.io",
    "rustup.rs",
    "doc.rust-lang.org",
    "dotnet.microsoft.com",
    "learn.microsoft.com",
    "aka.ms",
    "go.dev",
    "maven.apache.org",
    "www.powershellgallery.com",
    "getcomposer.org",
    "rubyinstaller.org",
    "guides.rubygems.org",
    "chocolatey.org",
    "docs.chocolatey.org",
    "scoop.sh",
    "docs.conda.io",
    "dart.dev",
    "luarocks.org",
    "www.cpan.org",
    "metacpan.org",
    "github.com",
    // 包仓库 / 包主页
    "www.npmjs.com",
    "pypi.org",
    "crates.io",
    "www.nuget.org",
    "winget.run",
    "packagist.org",
    "rubygems.org",
    "pkg.go.dev",
    "central.sonatype.com",
    "community.chocolatey.org",
    "anaconda.org",
    "pub.dev",
    // 「关于」页展示的项目与框架官网
    "tauri.app",
    "vuejs.org",
    "vite.dev",
    "www.rust-lang.org",
];

/// 校验外部链接：必须 https、无凭据、主机在白名单内
pub fn check_url(raw: &str) -> AppResult<String> {
    let url = raw.trim();
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return Err(AppError::forbidden("只允许打开 https 链接"));
    }
    if url.contains(char::is_whitespace) || url.contains(['\n', '\r', '"', '\'', '`', '<', '>']) {
        return Err(AppError::forbidden("链接包含非法字符"));
    }
    if lower.contains('@') && lower.split("://").nth(1).map(|r| r.contains('@')).unwrap_or(false) {
        return Err(AppError::forbidden("链接不能包含凭据信息"));
    }
    let host_port = lower.trim_start_matches("https://").split(['/', '?', '#']).next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("");
    if host.is_empty() {
        return Err(AppError::invalid("链接缺少主机名"));
    }
    let allowed = ALLOWED_HOSTS.iter().any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")));
    if !allowed {
        return Err(AppError::forbidden(format!("{host} 不在允许打开的域名列表内")));
    }
    Ok(url.to_string())
}

/// 用系统默认浏览器打开链接。
///
/// 安全：这是全项目**唯一**的 shell 调用点，因此：
/// - URL 必须先过 `check_url`（https + 域名白名单 + 无危险字符）
/// - 参数通过 `Command::arg` 传递，不经过字符串拼接
/// - 不打印、不记录完整 URL 到日志
pub fn open_external(url: &str) -> AppResult<String> {
    let safe = check_url(url)?;

    #[cfg(windows)]
    {
        // rundll32 是 Windows 官方推荐的「无 shell 打开 URL」方式，
        // 不像 `cmd /c start` 那样需要经过命令行解析。
        std::process::Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(&safe)
            .spawn()
            .map_err(|e| AppError::io(format!("打开浏览器失败: {e}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&safe)
            .spawn()
            .map_err(|e| AppError::io(format!("打开浏览器失败: {e}")))?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // 依次尝试，避免依赖某一个桌面环境
        let mut spawned = false;
        for opener in ["xdg-open", "gio", "sensible-browser"] {
            let mut cmd = std::process::Command::new(opener);
            if opener == "gio" {
                cmd.arg("open");
            }
            if cmd.arg(&safe).spawn().is_ok() {
                spawned = true;
                break;
            }
        }
        if !spawned {
            return Err(AppError::io("未找到可用的浏览器打开方式"));
        }
    }

    Ok(safe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_hosts_pass() {
        assert!(check_url("https://nodejs.org/en/download").is_ok());
        assert!(check_url("https://www.npmjs.com/package/vue").is_ok());
        assert!(check_url("https://pypi.org/project/requests").is_ok());
        assert!(check_url("https://sub.pypi.org/x").is_ok(), "子域名应通过");
        // 镜像站不是「打开」白名单的目标：镜像地址由 registry 模块单独处理
        assert!(check_url("https://registry.npmmirror.com").is_err());
    }

    #[test]
    fn rejects_dangerous_or_unknown_urls() {
        assert!(check_url("http://nodejs.org").is_err(), "必须 https");
        assert!(check_url("file:///C:/Windows/System32/calc.exe").is_err());
        assert!(check_url("https://evil.example.com/phish").is_err(), "未知域名应拒绝");
        assert!(check_url("https://nodejs.org/a b").is_err());
        assert!(check_url("https://user:pass@nodejs.org").is_err());
        assert!(check_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn settings_roundtrip_validates_values() {
        let mut s = AppSettings::default();
        assert_eq!(s.language, "zh-CN");
        s.language = "fr-FR".into();
        assert!(save(&s).is_err(), "不支持的语言必须被拒绝");
        s.language = "en-US".into();
        s.theme = "neon".into();
        assert!(save(&s).is_err(), "不支持的主题必须被拒绝");
    }

    #[test]
    fn settings_path_lives_under_user_config() {
        let path = settings_path().expect("应能定位配置目录");
        assert!(path.ends_with("settings.json"));
        assert!(path.to_string_lossy().contains("PackInspect"));
    }
}
