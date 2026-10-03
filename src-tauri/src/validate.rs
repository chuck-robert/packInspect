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

/// 校验用户输入的包名。空串、含空白、含引号、含 shell 元字符一律拒绝。
///
/// 【为什么必须拒绝前导 `-`】
/// 包名会被替换进白名单模板（如 `pip uninstall -y {}`）。若包名以 `-` 开头，
/// 它就会被目标程序当成**选项**而非参数 —— 例如
/// `pip uninstall -y --target=/etc/passwd x` 会把安装位置指到系统目录。
/// 模板里的参数位置是固定的，因此这里从入口堵死选项注入。
///
/// 注意：`*` `?` `[` 这些通配/正则元字符**已经**被字符集排除在外，
/// 这一点很关键 —— apt-get 会把不匹配的参数当 POSIX 正则匹配全部包名，
/// dnf/pacman 也会做 glob 展开。字符集是这道防线的主要手段。
pub fn package_name(name: &str) -> AppResult<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("包名不能为空"));
    }
    if trimmed.len() > 214 {
        // npm 包名长度上限
        return Err(AppError::invalid("包名过长"));
    }
    if trimmed.starts_with('-') {
        return Err(AppError::forbidden(format!(
            "包名不能以 - 开头（会被当成命令行选项）: {trimmed}"
        )));
    }
    if trimmed.starts_with('.') || trimmed.starts_with('/') || trimmed.contains("..") {
        return Err(AppError::forbidden(format!("包名包含非法路径片段: {trimmed}")));
    }
    if !trimmed.chars().all(is_name_char) {
        return Err(AppError::forbidden(format!("包名包含非法字符: {trimmed}")));
    }
    Ok(trimmed)
}

/// 系统级包管理器（apt / dnf / pacman / …）的**更严格**包名校验。
///
/// 【为什么这些生态需要单独一套】
/// 它们把包名当作**规格表达式**处理，而不只是字符串：
/// - `apt-get`：不匹配的参数只要含 `.` `?` `*` 就被当成 POSIX 正则，按**子串**匹配所有包名，
///   官方 man 页举的例子是 `lo.*` 会命中 `how-lo` 与 `lowest`。合法包名里本来就有 `.`，
///   所以这个回退路径是真实可达的。
/// - `dnf remove`：包规格支持 `*` `?` `[]`，且 dnf 会**自己展开**（连引号也挡不住）。
/// - `pacman`：包名可带 `group/` 前缀等特殊语法。
///
/// 这些生态的合法包名只会用到 `[a-z0-9][a-z0-9+._-]*`，不需要 `@` 与 `/`，
/// 因此这里收紧到该字符集 —— 从根上消灭正则/glob 被触发的可能。
pub fn system_package_name(name: &str) -> AppResult<&str> {
    let trimmed = package_name(name)?;
    let ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '_' | '-'));
    if !ok {
        return Err(AppError::forbidden(format!(
            "系统包管理器不接受该字符（可能被当作正则或通配符）: {trimmed}"
        )));
    }
    if trimmed.starts_with('.') {
        return Err(AppError::forbidden(format!("包名不能以 . 开头: {trimmed}")));
    }
    Ok(trimmed)
}

/// 校验在线搜索关键词。
///
/// 允许空格（多词搜索很常见），但禁止一切能改变 URL 结构、构成路径片段或注入 shell 的字符。
/// 刻意**不允许 `/`**：包名里的 `/` 只出现在 npm 作用域前缀（`@scope/name`）与 Go module 路径中，
/// 前者只需搜 `name` 即可命中，后者不在在线浏览支持范围内；
/// 而放行 `/` 会让 `../../etc/passwd` 这类输入通过校验。
pub fn search_query(query: &str) -> AppResult<&str> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("搜索关键词不能为空"));
    }
    if trimmed.chars().count() > 100 {
        return Err(AppError::invalid("搜索关键词过长（上限 100 字符）"));
    }
    if trimmed.contains("..") {
        return Err(AppError::forbidden("搜索关键词不能包含 .. 路径片段"));
    }
    let allowed = |c: char| {
        c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '+' | '@' | ':' | '(' | ')')
    };
    if !trimmed.chars().all(allowed) {
        let bad: String = trimmed.chars().filter(|c| !allowed(*c)).take(5).collect();
        return Err(AppError::forbidden(format!("搜索关键词包含非法字符: {bad}")));
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

/// 允许被执行的包管理操作。
///
/// 这三个操作会**改动用户真实环境**，因此用枚举收敛取值范围 ——
/// 前端只能提交这三个字面量之一，无法构造出别的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageOp {
    Update,
    Uninstall,
    Install,
}

impl PackageOp {
    pub fn parse(value: &str) -> AppResult<PackageOp> {
        match value.to_ascii_lowercase().as_str() {
            "update" => Ok(PackageOp::Update),
            "uninstall" => Ok(PackageOp::Uninstall),
            "install" => Ok(PackageOp::Install),
            other => Err(AppError::forbidden(format!("不允许的操作: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PackageOp::Update => "update",
            PackageOp::Uninstall => "uninstall",
            PackageOp::Install => "install",
        }
    }

    /// 卸载会移除软件，属于破坏性操作
    pub fn is_destructive(self) -> bool {
        matches!(self, PackageOp::Uninstall)
    }
}

/// 校验包管理器 id 是白名单内的已知项
pub fn validate_manager(manager: &str) -> AppResult<&str> {
    if crate::whitelist::find(manager).is_some() {
        Ok(manager)
    } else {
        Err(AppError::invalid(format!("不支持的包管理器: {manager}")))
    }
}

/// 校验 (包管理器, 包名, 操作) 组合，并返回**静态参数模板**。
///
/// 返回值里恰有一个元素是 `{}`，由调用方替换为已校验的包名。
/// 因此最终命令完全由 Rust 侧白名单决定，前端无法影响参数内容。
pub fn resolve_package_op(
    manager: &str,
    package: &str,
    op: PackageOp,
) -> AppResult<&'static [&'static str]> {
    validate_manager(manager)?;

    // 包名走哪一套校验由**生态**决定：系统级包管理器把包名当规格表达式处理，
    // 必须用更严格的字符集（见 `system_package_name` 的注释）。
    if crate::whitelist::uses_spec_syntax(manager) {
        system_package_name(package)?;
    } else {
        package_name(package)?;
    }

    crate::whitelist::op_args(manager, op.as_str())
        .ok_or_else(|| AppError::forbidden(format!("{manager} 不支持 {} 操作", op.as_str())))
}

/// 系统级包管理器的包名更严格：拒绝一切可能被当作**正则或通配符**的字符。
///
/// 为什么这批需要单独一套：apt-get 会把不匹配的参数当 POSIX 正则按子串匹配全部包名
/// （man 页的例子 `lo.*` 命中 `how-lo` 与 `lowest`），dnf 会对包规格做 glob 展开。
/// 这些生态的合法包名只用到 `[a-z0-9+._-]`，收紧到该字符集即可从根上消灭该风险。
#[cfg(test)]
mod system_name_tests {
    use super::*;

    #[test]
    fn accepts_real_system_package_names() {
        for good in [
            "bash",
            "libc6-dev",
            "g++",
            "python3.12",
            "gcc-13-base",
            "gtk+3.0",
            "libstdc++6",
        ] {
            assert!(system_package_name(good).is_ok(), "{good} 应被接受");
        }
    }

    #[test]
    fn rejects_regex_and_glob_metacharacters() {
        // 这些是 apt-get 正则回退与 dnf glob 展开的触发字符
        for bad in ["*", "?", "[abc]", "lo.*", "a*", "bash?", "^bash", "bash$", "a|b"] {
            assert!(
                system_package_name(bad).is_err(),
                "{bad:?} 含正则/通配元字符，必须拒绝（否则 apt-get 会按正则匹配全部包）"
            );
        }
    }

    #[test]
    fn rejects_option_injection_for_every_ecosystem() {
        // 前导 - 会被目标程序当成选项：pip uninstall -y --target=/etc x 之类
        for bad in [
            "--target=/etc/passwd",
            "-y",
            "--prefix",
            "--allow-downgrades",
            "--force-yes",
            "--allowerasing",
        ] {
            assert!(
                package_name(bad).is_err(),
                "{bad:?} 以 - 开头，会被当成选项，必须拒绝"
            );
            assert!(system_package_name(bad).is_err(), "{bad:?} 同样应被系统校验拒绝");
        }
    }

    #[test]
    fn ecosystem_scoped_packages_still_work() {
        // npm 作用域包是合法输入，不能因为收紧系统包名而误伤
        assert!(package_name("@anthropic-ai/claude-code").is_ok());
        // 但它们**不该**通过系统级校验（系统生态用不到 @ 与 /）
        assert!(system_package_name("@scope/pkg").is_err());
        assert!(system_package_name("a/b").is_err());
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
    fn search_query_allows_spaces_but_blocks_injection() {
        assert!(search_query("vue").is_ok());
        assert!(search_query("vue router").is_ok());
        assert!(search_query("@antfu").is_ok());
        assert!(search_query("Newtonsoft.Json").is_ok());
        assert!(search_query("org.slf4j:slf4j-api").is_ok());
        for bad in [
            "", "   ", "a;b", "a|b", "a&b", "a$(id)", "a`id`", "a\nb", "a>b", "a'b", "a\"b",
            "../../etc/passwd", "a/b", "a\\b", "x?y", "x#y",
        ] {
            assert!(search_query(bad).is_err(), "{bad:?} 应被拒绝");
        }
        // 超长关键词
        assert!(search_query(&"x".repeat(101)).is_err());
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
