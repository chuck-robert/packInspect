//! 包内子节点（插件 / 扩展 / 依赖 / 可执行文件）扫描。
//!
//! 用途：右键某个包时，展开它内部真正装了什么。
//! 每个生态的「插件」概念不同，所以统一抽象成 `PluginNode` 树：
//!
//! | 管理器 | 子节点含义 | 数据来源 |
//! |---|---|---|
//! | npm / pnpm / yarn | 依赖、声明的扩展点 | `node_modules/<dep>/package.json`、`contributes` |
//! | pip | 运行时依赖、顶层导入包 | `<pkg>.dist-info/METADATA`、`top_level.txt` |
//! | cargo | 随包安装的可执行文件 | `cargo install --list` 的产物行 |
//! | dotnet | 包内文件（lib / tools / build） | `.nupkg` 目录结构 |
//! | 其它 | 一级子目录 + 目录内主要文件 | 受限深度遍历 |

use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::manager;
use crate::models::{PackageRecord, PluginNode};
use crate::validate;
use std::path::{Path, PathBuf};

/// 展开的依赖上限 —— 一个包可能有几百个依赖，全展开会让界面失去可读性
const MAX_DEPENDENCIES: usize = 60;
/// 一级子目录展示上限
const MAX_CHILDREN: usize = 40;

/// 入口：为某个包收集子节点。
///
/// 安全：`path` 若存在，必须落在该管理器的全局根之内；否则直接拒绝，
/// 避免被诱导去遍历用户任意目录。
pub fn collect(record: &PackageRecord, timeout_ms: u64) -> AppResult<Vec<PluginNode>> {
    let mut nodes = Vec::new();

    // 1) 命令类来源（不依赖路径）：cargo 的产物文件
    if record.manager == "cargo" {
        nodes.extend(cargo_binaries(record, timeout_ms));
    }

    // 2) 路径类来源
    if let Some(raw) = record.path.as_deref() {
        let path = PathBuf::from(raw);
        if path.exists() {
            if let Some(guard) = boundary_for(&record.manager, timeout_ms) {
                // 只允许在管理器自己的目录树内展开
                if validate::ensure_within(&path, &guard).is_err() {
                    nodes.push(PluginNode {
                        node_type: "dir".into(),
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| raw.to_string()),
                        version: None,
                        path: Some(raw.to_string()),
                        size: None,
                        note: Some("路径不在该包管理器的已知目录内，已跳过内容展开".into()),
                    });
                    return Ok(nodes);
                }
            }
            // 包路径可能是 dist-info 目录（Python），其兄弟才是包本体
            let base = if record.manager == "pip" { dist_info_parent(&path) } else { path.clone() };
            nodes.extend(scan_by_manager(&record.manager, &base, record));
        }
    }

    // 去重（同名同路径只留一条），并保持稳定顺序
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| seen.insert((n.node_type.clone(), n.name.clone(), n.path.clone())));
    Ok(nodes)
}

/// 该管理器的「边界目录」：子节点路径必须落在其中
fn boundary_for(manager_id: &str, timeout_ms: u64) -> Option<PathBuf> {
    manager::global_root_for(manager_id, timeout_ms)
        .or_else(|| manager::cache_dir_for(manager_id, timeout_ms))
}

/// Python 的包体目录：`site-packages/`，即 dist-info 的父目录
fn dist_info_parent(dist_info: &Path) -> PathBuf {
    dist_info.parent().map(Path::to_path_buf).unwrap_or_else(|| dist_info.to_path_buf())
}

fn scan_by_manager(manager: &str, base: &Path, record: &PackageRecord) -> Vec<PluginNode> {
    match manager {
        "npm" | "pnpm" | "yarn" => node_children(base, record),
        "pip" => pip_children(base, record),
        "dotnet" => dotnet_children(base),
        _ => generic_children(base),
    }
}

// ---------------------------------------------------------------------------
// Node：依赖 + 声明的扩展点
// ---------------------------------------------------------------------------

/// 读取 `package.json` 里的 `contributes` / `activationEvents` /
/// `extensionDependencies`，这些是「插件」语义最直接的体现。
fn node_children(base: &Path, record: &PackageRecord) -> Vec<PluginNode> {
    let mut nodes = Vec::new();
    let pkg_json = base.join("package.json");
    let Ok(Some(value)) = fsutil::read_json(&pkg_json, 512 * 1024) else {
        return nodes;
    };

    // 1) 依赖节点：逐个到 node_modules 里找实际安装的版本
    let mut dep_count = 0;
    for section in ["dependencies", "peerDependencies", "optionalDependencies"] {
        let Some(map) = value.get(section).and_then(|v| v.as_object()) else { continue };
        for (dep_name, wanted) in map {
            if dep_count >= MAX_DEPENDENCIES {
                break;
            }
            dep_count += 1;
            let dep_dir = base.join("node_modules").join(dep_name);
            let installed = if dep_dir.is_dir() {
                fsutil::read_json(&dep_dir.join("package.json"), 128 * 1024)
                    .ok()
                    .flatten()
                    .and_then(|v| v.get("version").and_then(|s| s.as_str()).map(str::to_string))
            } else {
                None
            };
            nodes.push(PluginNode {
                node_type: "dependency".into(),
                name: dep_name.clone(),
                version: installed.clone().or_else(|| wanted.as_str().map(str::to_string)),
                path: if dep_dir.is_dir() { Some(dep_dir.to_string_lossy().to_string()) } else { None },
                size: None,
                note: Some(match (&installed, section) {
                    (Some(v), _) => format!("已安装 {v}（{section}）"),
                    (None, s) => format!("声明于 {s}，未随包安装"),
                }),
            });
        }
    }

    // 2) 扩展点：`contributes` 是 VS Code 类插件的核心字段
    if let Some(contributes) = value.get("contributes").and_then(|v| v.as_object()) {
        let kinds: Vec<String> = contributes.keys().cloned().collect();
        if !kinds.is_empty() {
            nodes.push(PluginNode {
                node_type: "plugin".into(),
                name: format!("扩展点 ×{}", kinds.len()),
                version: None,
                path: Some(pkg_json.to_string_lossy().to_string()),
                size: None,
                note: Some(format!("contributes: {}", kinds.join(", "))),
            });
        }
    }

    if let Some(events) = value.get("activationEvents").and_then(|v| v.as_array()) {
        if !events.is_empty() {
            nodes.push(PluginNode {
                node_type: "plugin".into(),
                name: format!("激活事件 ×{}", events.len()),
                version: None,
                path: None,
                size: None,
                note: Some(
                    events
                        .iter()
                        .take(8)
                        .filter_map(|e| e.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            });
        }
    }

    // 3) 入口文件与可执行命令
    if let Some(bin) = value.get("bin") {
        let commands: Vec<String> = match bin {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Object(o) => o.keys().cloned().collect(),
            _ => Vec::new(),
        };
        if !commands.is_empty() {
            let label = record.name.rsplit('/').next().unwrap_or(&record.name).to_string();
            nodes.push(PluginNode {
                node_type: "runtime".into(),
                name: format!("命令 {label}"),
                version: None,
                path: bin
                    .as_object()
                    .and_then(|o| o.values().next())
                    .and_then(|v| v.as_str())
                    .map(|rel| base.join(rel).to_string_lossy().to_string()),
                size: None,
                note: Some(format!("bin: {}", commands.join(", "))),
            });
        }
    }

    nodes
}

// ---------------------------------------------------------------------------
// Python：运行时依赖 + 顶层导入包
// ---------------------------------------------------------------------------

/// 解析 `METADATA` 里的 `Requires-Dist`，以及 `top_level.txt`
fn pip_children(site_packages: &Path, record: &PackageRecord) -> Vec<PluginNode> {
    let mut nodes = Vec::new();

    // 定位该包自己的 dist-info（record.path 可能已经是 dist-info）
    let dist_info = record
        .path
        .as_deref()
        .map(PathBuf::from)
        .filter(|p| p.file_name().map(|n| n.to_string_lossy().ends_with(".dist-info")).unwrap_or(false))
        .or_else(|| find_dist_info(site_packages, &record.name));

    if let Some(di) = &dist_info {
        // METADATA → Requires-Dist
        let metadata = di.join("METADATA");
        if let Ok(text) = fsutil::read_text(&metadata, 1024 * 1024) {
            let mut count = 0;
            for line in text.lines() {
                if count >= MAX_DEPENDENCIES {
                    break;
                }
                let Some(rest) = line.strip_prefix("Requires-Dist:") else { continue };
                let spec = rest.trim();
                if spec.is_empty() {
                    continue;
                }
                // 跳过仅在 extra 里启用的可选依赖，减少噪音
                if spec.contains("extra ==") {
                    continue;
                }
                count += 1;
                // `name (>=1.0) ; extra == 'x'` → 包名
                let name = spec
                    .split([' ', '(', ';', '<', '>', '=', '!', '~', '['])
                    .next()
                    .unwrap_or(spec)
                    .trim()
                    .to_string();
                let constraint = spec
                    .split_once('(')
                    .and_then(|(_, r)| r.split_once(')'))
                    .map(|(v, _)| v.trim().to_string());
                nodes.push(PluginNode {
                    node_type: "dependency".into(),
                    name,
                    version: constraint,
                    path: None,
                    size: None,
                    note: Some(format!("Requires-Dist: {spec}")),
                });
            }
        }

        // top_level.txt → 实际安装的顶层导入名
        let top_level = di.join("top_level.txt");
        if let Ok(text) = fsutil::read_text(&top_level, 64 * 1024) {
            for name in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(10) {
                let mod_dir = site_packages.join(name);
                nodes.push(PluginNode {
                    node_type: "dir".into(),
                    name: name.to_string(),
                    version: None,
                    path: if mod_dir.exists() { Some(mod_dir.to_string_lossy().to_string()) } else { None },
                    size: None,
                    note: Some("顶层导入模块".into()),
                });
            }
        }

        // RECORD 里的文件（只给个数量提示，不逐条列出）
        if let Ok(text) = fsutil::read_text(&di.join("RECORD"), 2 * 1024 * 1024) {
            let files = text.lines().filter(|l| !l.trim().is_empty()).count();
            nodes.push(PluginNode {
                node_type: "file".into(),
                name: format!("安装文件 ×{files}"),
                version: None,
                path: Some(di.to_string_lossy().to_string()),
                size: None,
                note: Some("来自 RECORD".into()),
            });
        }
    }

    // 嵌套的 `*.dist-info`（命名空间包/子包）
    if let Ok(entries) = std::fs::read_dir(site_packages) {
        let prefix = record.name.to_ascii_lowercase().replace('-', "_");
        for entry in entries.filter_map(Result::ok).take(4000) {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let lower = dir_name.to_ascii_lowercase();
            if lower.ends_with(".dist-info")
                && lower.starts_with(&prefix)
                && dist_info.as_ref().map(|d| d != &entry.path()).unwrap_or(true)
            {
                nodes.push(PluginNode {
                    node_type: "dir".into(),
                    name: dir_name,
                    version: None,
                    path: Some(entry.path().to_string_lossy().to_string()),
                    size: None,
                    note: Some("同名前缀的元数据目录".into()),
                });
            }
        }
    }

    nodes
}

fn find_dist_info(site_packages: &Path, package: &str) -> Option<PathBuf> {
    let prefix = package.to_ascii_lowercase().replace('-', "_");
    let entries = std::fs::read_dir(site_packages).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".dist-info") && (name.starts_with(&prefix) || name.starts_with(&package.to_ascii_lowercase())) {
            return Some(entry.path());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// .NET：包内文件结构
// ---------------------------------------------------------------------------

/// NuGet 包的典型目录：`lib/`、`tools/`、`build/`、`contentFiles/`、`runtimes/`
fn dotnet_children(base: &Path) -> Vec<PluginNode> {
    let mut nodes = Vec::new();
    let Ok(entries) = std::fs::read_dir(base) else { return nodes };

    for entry in entries.filter_map(Result::ok).take(MAX_CHILDREN) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // .nuspec 是包清单，单独标注
        let is_nuspec = name.to_ascii_lowercase().ends_with(".nuspec");
        let node_type = if path.is_dir() { "dir" } else { "file" };
        let note = match name.as_str() {
            "lib" => Some("可引用的程序集（按目标框架分目录）".to_string()),
            "tools" => Some("包内可执行工具".to_string()),
            "build" => Some("MSBuild 目标与属性".to_string()),
            "contentFiles" => Some("内容文件".to_string()),
            "runtimes" => Some("本机运行时资源".to_string()),
            "analyzers" => Some("Roslyn 分析器".to_string()),
            _ if is_nuspec => Some("NuGet 包清单".to_string()),
            _ => None,
        };
        // 目录只算一层大小，避免深挖整个包体
        let size = if path.is_file() { entry.metadata().ok().map(|m| m.len()) } else { None };
        nodes.push(PluginNode {
            node_type: node_type.into(),
            name,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            size,
            note,
        });
    }
    nodes
}

// ---------------------------------------------------------------------------
// cargo：随包安装的可执行文件
// ---------------------------------------------------------------------------

/// cargo 的包本体在源码缓存里，用户真正关心的是「装了哪些命令到 ~/.cargo/bin」。
/// 直接解析 `cargo install --list` 的产物行（缩进的 `xxx.exe`）最准确。
fn cargo_binaries(record: &PackageRecord, timeout_ms: u64) -> Vec<PluginNode> {
    let Some(text) = crate::packages::run_whitelisted("cargo", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    let mut current: Option<String> = None;

    for raw in text.lines() {
        // 包名行：`    name vX.Y.Z:`
        if let Some(head) = raw.trim().strip_suffix(':') {
            if let Some((name, _)) = head.trim().rsplit_once(" v") {
                current = Some(name.trim().to_string());
            }
            continue;
        }
        // 产物行：缩进的文件名
        let trimmed = raw.trim();
        if trimmed.is_empty() || !raw.starts_with([' ', '\t']) {
            continue;
        }
        let belongs = current.as_deref() == Some(record.name.as_str());
        if !belongs {
            continue;
        }
        // 优先展示 .exe，其次是无扩展名的 unix 产物
        if trimmed.ends_with(".exe") || !trimmed.contains('.') {
            nodes.push(PluginNode {
                node_type: "runtime".into(),
                name: trimmed.to_string(),
                version: record.version.clone(),
                path: None,
                size: None,
                note: Some("已安装到 cargo bin 目录的命令".into()),
            });
        }
    }
    nodes
}

// ---------------------------------------------------------------------------
// 通用：一级子目录 + 主要文件
// ---------------------------------------------------------------------------

fn generic_children(base: &Path) -> Vec<PluginNode> {
    let mut nodes = Vec::new();
    let Ok(entries) = std::fs::read_dir(base) else { return nodes };
    let mut children: Vec<(String, PathBuf, bool, u64)> = entries
        .filter_map(Result::ok)
        .take(MAX_CHILDREN)
        .map(|e| {
            let path = e.path();
            let is_dir = path.is_dir();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.file_name().to_string_lossy().to_string(), path, is_dir, size)
        })
        .collect();
    // 目录在前，其次按名字排序，便于对照
    children.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));

    for (name, path, is_dir, size) in children {
        nodes.push(PluginNode {
            node_type: if is_dir { "dir".into() } else { "file".into() },
            name,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            size: if is_dir { None } else { Some(size) },
            note: None,
        });
    }
    nodes
}

/// 校验包名后再展开（命令层调用这个）
pub fn collect_checked(
    manager_id: &str,
    package: &str,
    path: Option<String>,
    version: Option<String>,
    timeout_ms: u64,
) -> AppResult<Vec<PluginNode>> {
    if crate::whitelist::find(manager_id).is_none() {
        return Err(AppError::invalid(format!("不支持的包管理器: {manager_id}")));
    }
    validate::package_name(package)?;
    let record = PackageRecord {
        name: package.to_string(),
        version,
        manager: manager_id.to_string(),
        scope: "global".to_string(),
        path,
        size: None,
        redundant: false,
        redundant_reason: None,
        description: None,
        latest_version: None,
        plugins: Vec::new(),
        plugins_loaded: false,
        icon: None,
    };
    collect(&record, timeout_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PackageRecord;

    fn record_with(manager: &str, name: &str, path: Option<&str>) -> PackageRecord {
        PackageRecord {
            name: name.into(),
            version: Some("1.0.0".into()),
            manager: manager.into(),
            scope: "global".into(),
            path: path.map(str::to_string),
            size: None,
            redundant: false,
            redundant_reason: None,
            description: None,
            latest_version: None,
            plugins: Vec::new(),
            plugins_loaded: false,
            icon: None,
        }
    }

    #[test]
    fn rejects_unknown_manager_and_bad_package_name() {
        assert!(collect_checked("nope", "vue", None, None, 1000).is_err());
        assert!(collect_checked("npm", "vue; rm -rf /", None, None, 1000).is_err());
    }

    #[test]
    fn node_children_reads_dependencies_and_contributes() {
        let dir = std::env::temp_dir().join(format!("pi-plugins-{}", std::process::id()));
        let pkg = dir.join("pkg");
        std::fs::create_dir_all(pkg.join("node_modules/semver")).unwrap();
        std::fs::write(
            pkg.join("package.json"),
            r#"{
              "name": "demo",
              "dependencies": {"semver": "^7.0.0"},
              "contributes": {"commands": [], "menus": {}},
              "activationEvents": ["onStartupFinished"],
              "bin": {"demo": "./cli.js"}
            }"#,
        )
        .unwrap();
        std::fs::write(pkg.join("node_modules/semver/package.json"), r#"{"name":"semver","version":"7.6.0"}"#).unwrap();

        let record = record_with("npm", "demo", Some(&pkg.to_string_lossy()));
        let nodes = node_children(&pkg, &record);
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"semver"), "应列出依赖 semver：{names:?}");
        assert!(nodes.iter().any(|n| n.node_type == "plugin"), "应识别 contributes 扩展点");
        assert!(nodes.iter().any(|n| n.node_type == "runtime"), "应识别 bin 命令");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pip_children_reads_requires_dist() {
        let dir = std::env::temp_dir().join(format!("pi-pip-{}", std::process::id()));
        let site = dir.join("site-packages");
        let di = site.join("demo-1.0.0.dist-info");
        std::fs::create_dir_all(&di).unwrap();
        std::fs::write(
            di.join("METADATA"),
            "Metadata-Version: 2.1\nName: demo\nRequires-Dist: requests (>=2.0)\nRequires-Dist: pytest ; extra == 'test'\n",
        )
        .unwrap();
        std::fs::write(di.join("top_level.txt"), "demo\n").unwrap();
        std::fs::write(di.join("RECORD"), "demo/__init__.py,sha256=abc,12\n").unwrap();

        let record = record_with("pip", "demo", Some(&di.to_string_lossy()));
        let nodes = pip_children(&site, &record);
        // 只应保留非 extra 的那条依赖
        let deps: Vec<&str> = nodes
            .iter()
            .filter(|n| n.node_type == "dependency")
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(deps, vec!["requests"], "应只保留 requests，过滤掉 extra 依赖");
        assert!(nodes.iter().any(|n| n.name == "demo"), "应识别顶层模块");
        assert!(nodes.iter().any(|n| n.name.starts_with("安装文件")), "应统计 RECORD");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dotnet_children_labels_known_dirs() {
        let dir = std::env::temp_dir().join(format!("pi-nuget-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::create_dir_all(dir.join("tools")).unwrap();
        std::fs::write(dir.join("demo.nuspec"), "<package/>").unwrap();

        let nodes = dotnet_children(&dir);
        let lib = nodes.iter().find(|n| n.name == "lib").unwrap();
        assert!(lib.note.as_deref().unwrap().contains("程序集"));
        assert!(nodes.iter().any(|n| n.name.ends_with(".nuspec")));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
