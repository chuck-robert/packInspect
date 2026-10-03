//! 包管理器注册表 + 命令白名单。
//!
//! 【安全核心】前端永远不能传命令行字符串，只能传 `{ manager, op }`。
//! 这里定义每个 (manager, op) 对应的**静态参数数组**，Rust 侧据此拼装 `Command`。
//! 参数里出现的一切动态值（包名、路径）都必须先过 `validate` 校验。

/// 一个包管理器的静态定义
pub struct ManagerDef {
    pub id: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    /// 候选可执行文件名（按优先级），用于在 PATH 中查找
    pub exe_candidates: &'static [&'static str],
    /// 用户级配置文件相对 home 的路径候选
    pub config_files: &'static [&'static str],
    /// 该管理器允许执行的操作 → 静态参数数组
    pub ops: &'static [(&'static str, &'static [&'static str])],
    /// 清理时**允许删除**的路径推导规则（见 cleaner.rs 的 CacheRule）
    pub cache_subdirs: &'static [&'static str],
    /// 是否绝对禁止删除其缓存根（例如 pnpm store / cargo registry 删除代价过大）
    pub cache_root_protected: bool,
}

/// 本机支持扫描的全部包管理器。
///
/// 说明：`exe_candidates` 允许别名，例如 pip 在 Windows 上常以 `pip.exe` / `pip3.exe` 存在。
pub static MANAGERS: &[ManagerDef] = &[
    ManagerDef {
        id: "npm",
        name: "npm",
        language: "node",
        exe_candidates: &["npm.cmd", "npm.exe", "npm"],
        config_files: &[".npmrc"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["ls", "-g", "--depth=0", "--json", "--long=false"]),
            ("rootGlobal", &["root", "-g"]),
            ("prefixGlobal", &["prefix", "-g"]),
            ("cacheDir", &["config", "get", "cache"]),
        ],
        cache_subdirs: &["_cacache", "_npx", "_logs", "_update-notifier"],
        cache_root_protected: false,
    },
    ManagerDef {
        id: "pnpm",
        name: "pnpm",
        language: "node",
        exe_candidates: &["pnpm.cmd", "pnpm.exe", "pnpm"],
        config_files: &[".npmrc"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["ls", "-g", "--depth=0", "--json"]),
            ("rootGlobal", &["root", "-g"]),
            ("storePath", &["store", "path"]),
            ("cacheDir", &["store", "path"]),
        ],
        cache_subdirs: &["v3/files", "v10/files", "metadata", "metadata-full", "metadata-v1.3"],
        // pnpm store 是内容寻址仓库，硬链接自它的项目会受影响，删除需谨慎
        cache_root_protected: true,
    },
    ManagerDef {
        id: "yarn",
        name: "yarn",
        language: "node",
        exe_candidates: &["yarn.cmd", "yarn.exe", "yarn"],
        config_files: &[".yarnrc", ".yarnrc.yml"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["global", "list", "--json", "--depth=0"]),
            ("rootGlobal", &["global", "dir"]),
            ("cacheDir", &["cache", "dir"]),
        ],
        cache_subdirs: &["v6", "v4"],
        cache_root_protected: false,
    },
    ManagerDef {
        id: "bun",
        name: "bun",
        language: "node",
        exe_candidates: &["bun.exe", "bun"],
        config_files: &["bunfig.toml", ".npmrc"],
        ops: &[("version", &["--version"]), ("listGlobal", &["pm", "ls", "-g", "--json"])],
        cache_subdirs: &["install", "cache"],
        cache_root_protected: false,
    },
    ManagerDef {
        id: "pip",
        name: "pip",
        language: "python",
        exe_candidates: &["pip.exe", "pip3.exe", "pip", "pip3"],
        config_files: &["pip/pip.ini", "pip/pip.conf", ".pip/pip.conf", ".config/pip/pip.conf"],
        ops: &[("version", &["--version"]), ("listGlobal", &["list", "--format=json", "--disable-pip-version-check"])],
        cache_subdirs: &["http", "http-v2", "wheels", "selfcheck"],
        cache_root_protected: false,
    },
    ManagerDef {
        id: "uv",
        name: "uv",
        language: "python",
        exe_candidates: &["uv.exe", "uv"],
        config_files: &["uv/uv.toml", ".config/uv/uv.toml"],
        ops: &[
            ("version", &["--version"]),
            ("listGlobal", &["pip", "list", "--format=json"]),
            ("cacheDir", &["cache", "dir"]),
        ],
        cache_subdirs: &[],
        cache_root_protected: true,
    },
    ManagerDef {
        id: "cargo",
        name: "cargo",
        language: "rust",
        exe_candidates: &["cargo.exe", "cargo"],
        config_files: &["cargo/config.toml", ".cargo/config.toml", ".cargo/config"],
        ops: &[("version", &["--version"]), ("listGlobal", &["install", "--list"])],
        cache_subdirs: &["registry/cache", "registry/index", "git/db"],
        cache_root_protected: true,
    },
    ManagerDef {
        id: "go",
        name: "go",
        language: "go",
        exe_candidates: &["go.exe", "go"],
        config_files: &[".config/go/env"],
        ops: &[
            ("version", &["version"]),
            ("listGlobal", &["version", "-m"]),
            ("cacheDir", &["env", "GOMODCACHE"]),
        ],
        cache_subdirs: &["cache/download"],
        cache_root_protected: true,
    },
    ManagerDef {
        id: "gem",
        name: "gem",
        language: "ruby",
        exe_candidates: &["gem.cmd", "gem.exe", "gem"],
        config_files: &[".gemrc"],
        ops: &[("version", &["--version"]), ("listGlobal", &["list", "--local", "--no-versions"])],
        cache_subdirs: &["cache"],
        cache_root_protected: false,
    },
];

pub fn find(id: &str) -> Option<&'static ManagerDef> {
    MANAGERS.iter().find(|m| m.id == id)
}

/// 取出某个操作的白名单参数；返回 None 表示该操作不被允许。
pub fn op_args(id: &str, op: &str) -> Option<&'static [&'static str]> {
    find(id)?.ops.iter().find(|(name, _)| *name == op).map(|(_, args)| *args)
}

/// 允许被执行的**全部**操作名（用于错误提示与文档生成）
pub fn allowed_ops(id: &str) -> Vec<&'static str> {
    find(id).map(|m| m.ops.iter().map(|(n, _)| *n).collect()).unwrap_or_default()
}

/// 清理白名单：只有出现在这里的 (manager, kind) 组合才可能产生可删除候选。
/// 任何未列出的组合一律标记为 protected。
pub fn clean_allowed(manager: &str, kind: &str) -> bool {
    matches!(
        (manager, kind),
        ("npm", "cache")
            | ("npm", "temp")
            | ("pnpm", "cache")
            | ("yarn", "cache")
            | ("bun", "cache")
            | ("pip", "cache")
            | ("uv", "cache")
            | ("cargo", "cache")
            | ("go", "cache")
            | ("gem", "cache")
    )
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
    fn unknown_op_is_rejected() {
        assert!(op_args("npm", "execAnything").is_none());
        assert!(op_args("nonexistent", "version").is_none());
    }
}
