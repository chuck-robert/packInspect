//! 输入校验与路径守卫。
//!
//! 这是「禁止执行任意高危命令」和「不误删用户文件」的第二道防线：
//! 即使前端被注入，动态值也必须通过这里才能进入 `Command` 或 `remove_dir_all`。

use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};

/// 包名 / 版本 / 作用域名的合法字符集。
/// 覆盖 npm（`@scope/name`）、PyPI（`name-1.0.0`）、RubyGems、Go module 等命名习惯。
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '@' | '/' | '~')
}

/// 校验包名。空串、含空白、含引号、含 shell 元字符一律拒绝。
pub fn package_name(name: &str) -> AppResult<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("包名不能为空"));
    }
    if trimmed.len() > 214 {
        // npm 包名长度上限
        return Err(AppError::invalid("包名过长"));
    }
    if trimmed.starts_with('.') || trimmed.starts_with('/') || trimmed.contains("..") {
        return Err(AppError::forbidden(format!("包名包含非法路径片段: {trimmed}")));
    }
    if !trimmed.chars().all(is_name_char) {
        return Err(AppError::forbidden(format!("包名包含非法字符: {trimmed}")));
    }
    Ok(trimmed)
}

/// 校验用户输入的镜像源 URL：只允许 http/https，禁止凭据与本地文件协议
pub fn registry_url(url: &str) -> AppResult<&str> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(AppError::invalid("镜像源地址必须以 http:// 或 https:// 开头"));
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(AppError::forbidden("镜像源地址不能包含空白字符"));
    }
    // 防止写入配置时注入换行/注释符导致配置被污染
    if trimmed.contains(['\n', '\r', '#', ';']) {
        return Err(AppError::forbidden("镜像源地址包含非法字符"));
    }
    Ok(trimmed)
}

/// 允许写入的配置键白名单（按管理器）
pub fn config_key_allowed(manager: &str, key: &str) -> bool {
    match manager {
        "npm" | "pnpm" | "yarn" | "bun" => {
            matches!(key, "registry" | "strict-ssl" | "proxy" | "https-proxy" | "@scope:registry")
        }
        "pip" | "uv" => matches!(key, "index-url" | "extra-index-url" | "trusted-host" | "timeout"),
        "cargo" => matches!(key, "replace-with" | "registry" | "index"),
        "go" => matches!(key, "GOPROXY" | "GOSUMDB" | "GONOSUMDB"),
        "gem" => matches!(key, ":sources" | "sources"),
        _ => false,
    }
}

/// 展开用户输入路径中的 `~`
pub fn expand_tilde(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with(['/', '\\']) {
            if let Some(home) = home_dir() {
                let rest = rest.trim_start_matches(['/', '\\']);
                return if rest.is_empty() { home } else { home.join(rest) };
            }
        }
    }
    PathBuf::from(trimmed)
}

/// 取用户主目录。优先读环境变量：在中文用户名（非 ASCII）下 `dirs` 的已知文件夹查询可能失败。
pub fn home_dir() -> Option<PathBuf> {
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Some(PathBuf::from(v));
            }
        }
    }
    dirs::home_dir()
}

/// 取目录：环境变量优先，回落 `dirs`
pub fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty()).map(PathBuf::from)
}

/// 近似规范化路径（不要求路径已存在）。
///
/// 为何不用 `canonicalize`：清理候选可能已被外部删除，此时 canonicalize 会失败，
/// 而我们仍需要判断它是否位于允许的缓存根之下。这里手工消除 `.` 与 `..`。
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                // 已经到根就忽略，避免 "C:\.." 逃逸
                if !out.pop() {
                    out.push(Component::RootDir.as_os_str());
                }
            }
            Component::Normal(n) => out.push(n),
        }
    }
    out
}

/// 大小写不敏感的路径比较（Windows） / 敏感（其它平台）
fn path_starts_with(candidate: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        let c = candidate.to_string_lossy().to_ascii_lowercase();
        let r = root.to_string_lossy().to_ascii_lowercase();
        c == r || c.starts_with(&format!("{r}\\")) || c.starts_with(&format!("{r}/"))
    } else {
        candidate.starts_with(root)
    }
}

/// 路径守卫：`target` 必须严格位于 `root` 之内（不含 root 本身）。
///
/// 用于清理操作 —— 任何逃逸尝试（`..`、绝对路径、符号链接指向外部）都会被拒绝。
pub fn ensure_within(target: &Path, root: &Path) -> AppResult<PathBuf> {
    let root_abs = normalize(root);
    if !root_abs.is_absolute() {
        return Err(AppError::forbidden(format!("缓存根不是绝对路径: {}", root.display())));
    }
    let target_abs = if target.is_absolute() { normalize(target) } else { normalize(&root_abs.join(target)) };

    if !path_starts_with(&target_abs, &root_abs) || target_abs == root_abs {
        return Err(AppError::forbidden(format!(
            "拒绝操作：{} 不在允许的缓存目录 {} 之内",
            target_abs.display(),
            root_abs.display()
        )));
    }

    // 若路径已存在，进一步用 canonicalize 识破符号链接/junction 逃逸
    if target_abs.exists() {
        if let (Ok(real_target), Ok(real_root)) = (target_abs.canonicalize(), root_abs.canonicalize()) {
            if !path_starts_with(&real_target, &real_root) {
                return Err(AppError::forbidden(format!(
                    "拒绝操作：{} 实际指向 {}，超出缓存目录",
                    target_abs.display(),
                    real_target.display()
                )));
            }
        }
    }

    // 硬性拒绝删除盘符根、系统目录、用户主目录本身
    if is_critical_path(&target_abs) {
        return Err(AppError::forbidden(format!("拒绝操作关键路径: {}", target_abs.display())));
    }

    Ok(target_abs)
}

/// 绝不允许删除的路径（即使配置被改坏）
pub fn is_critical_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches(['\\', '/']).to_ascii_lowercase();
    if trimmed.is_empty() {
        return true;
    }
    // 盘符根，如 "c:"
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        return true;
    }
    let criticals: Vec<String> = [
        std::env::var("SystemRoot").ok(),
        std::env::var("windir").ok(),
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
    ]
    .into_iter()
    .flatten()
    .map(|v| v.to_ascii_lowercase())
    .collect();
    if criticals.iter().any(|c| !c.is_empty() && (trimmed == *c || trimmed.starts_with(&format!("{c}\\")))) {
        return true;
    }
    for key in ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim_end_matches(['\\', '/']).to_ascii_lowercase();
            if !v.is_empty() && trimmed == v {
                return true;
            }
        }
    }
    if let Some(home) = home_dir() {
        if trimmed == home.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase() {
            return true;
        }
    }
    false
}

/// 校验清理候选 id 只含安全字符（形如 `npm:cache:1a2b3c`）
pub fn candidate_id(id: &str) -> AppResult<&str> {
    let ok = !id.is_empty()
        && id.len() <= 256
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'));
    if ok {
        Ok(id)
    } else {
        Err(AppError::invalid(format!("非法的候选 id: {id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_package_names() {
        for n in ["vue", "@vue/cli", "requests", "torch-2.1.0", "lodash.merge", "github.com/spf13/cobra"] {
            assert!(package_name(n).is_ok(), "{n} 应被接受");
        }
    }

    #[test]
    fn rejects_injection_attempts() {
        for n in ["vue; rm -rf /", "$(whoami)", "`id`", "a b", "../../etc/passwd", "a|b", "a>b", "a\nb"] {
            assert!(package_name(n).is_err(), "{n} 应被拒绝");
        }
    }

    #[test]
    fn registry_url_only_http() {
        assert!(registry_url("https://registry.npmmirror.com").is_ok());
        assert!(registry_url("file:///etc/passwd").is_err());
        assert!(registry_url("https://a.com\ninjected=1").is_err());
    }

    #[test]
    fn ensure_within_blocks_escape() {
        let root = PathBuf::from("C:/Users/u/AppData/Local/npm-cache");
        assert!(ensure_within(Path::new("C:/Users/u/AppData/Local/npm-cache/_cacache"), &root).is_ok());
        assert!(ensure_within(Path::new("C:/Users/u/AppData/Local/npm-cache/../../../Windows"), &root).is_err());
        assert!(ensure_within(Path::new("C:/Windows/System32"), &root).is_err());
        // 不允许直接删缓存根本身
        assert!(ensure_within(&root, &root).is_err());
    }

    #[test]
    fn normalizes_parent_segments() {
        let p = normalize(Path::new("C:/a/b/../c/./d"));
        assert_eq!(p.to_string_lossy().replace('\\', "/"), "C:/a/c/d");
    }
}
