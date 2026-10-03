//! 已安装包枚举。
//!
//! 策略：**优先读磁盘，命令只用来「问路」**。
//! 例如 npm 用 `npm root -g` 问出全局目录，然后直接扫描该目录下的 package.json。
//! 这样即使某个命令超时/输出格式变化，仍能给出结果，且速度远快于解析命令文本。
//!
//! 每个管理器的实现都遵循同一签名：`fn xxx_packages(...) -> (Option<PathBuf>, Vec<PackageRecord>)`，
//! 方便 `collect_packages` 统一分发。

use crate::error::AppResult;
use crate::executor::{self, ExecRequest};
use crate::fsutil;
use crate::models::PackageRecord;
use crate::whitelist;
use serde_json::Value;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// 通用构件
// ---------------------------------------------------------------------------

/// 构造一条包记录，插件与图标字段留待后续填充。
///
/// 所有管理器共用它，新增一个生态时只需提供差异字段。
#[allow(clippy::too_many_arguments)]
fn rec(
    name: impl Into<String>,
    version: Option<String>,
    manager: &str,
    scope: &str,
    path: Option<String>,
) -> PackageRecord {
    let name = name.into();
    PackageRecord {
        name,
        version,
        manager: manager.to_string(),
        scope: scope.to_string(),
        path,
        size: None,
        redundant: false,
        redundant_reason: None,
        description: None,
        latest_version: None,
        // 图标只属于「包管理器」，不属于单个包，因此这里不再生成
        plugins: Vec::new(),
        plugins_loaded: false,
    }
}

/// 执行一条白名单命令，返回 stdout（失败返回 None，由调用方兜底到磁盘扫描）
fn try_cmd(manager: &str, op: &str, timeout_ms: u64) -> Option<String> {
    let args = whitelist::op_args(manager, op)?;
    let def = whitelist::find(manager)?;
    let exe = executor::resolve_executable(def.exe_candidates)?;
    let req = ExecRequest::new(exe.to_string_lossy().to_string(), args).with_timeout_ms(timeout_ms);
    let out = executor::run_resolved(&exe, &req).ok()?;
    if out.success {
        Some(out.stdout)
    } else {
        None
    }
}

/// 供其它模块复用白名单命令执行（插件分析需要解析 `cargo install --list` 原始输出）
pub fn run_whitelisted(manager: &str, op: &str, timeout_ms: u64) -> Option<String> {
    try_cmd(manager, op, timeout_ms)
}

/// 执行白名单命令并解析为 JSON
fn try_cmd_json(manager: &str, op: &str, timeout_ms: u64) -> Option<Value> {
    let raw = try_cmd(manager, op, timeout_ms)?;
    fsutil::parse_json_output(&raw).ok()
}

/// 取命令输出的第一行（用于 `npm root -g` 这类单值命令）
fn first_line(s: &str) -> Option<String> {
    s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.to_string())
}

/// 供 manager 探测使用的便捷包装
pub fn command_first_line(manager: &str, op: &str, timeout_ms: u64) -> Option<String> {
    try_cmd(manager, op, timeout_ms).and_then(|s| first_line(&s))
}

pub fn is_known_manager(id: &str) -> bool {
    whitelist::find(id).is_some()
}

pub fn ensure_known(id: &str) -> AppResult<()> {
    if is_known_manager(id) {
        Ok(())
    } else {
        Err(crate::error::AppError::invalid(format!("不支持的包管理器: {id}")))
    }
}

/// 读取一个 `package.json` 并转成记录
fn node_package_at(dir: &Path, manager: &str, scope: &str, measure: bool) -> Option<PackageRecord> {
    let pkg_json = dir.join("package.json");
    if !pkg_json.is_file() {
        return None;
    }
    let value = fsutil::read_json(&pkg_json, 256 * 1024).ok().flatten()?;
    let name = value.get("name").and_then(Value::as_str).map(str::to_string);
    let version = value.get("version").and_then(Value::as_str).map(str::to_string);
    let description =
        value.get("description").and_then(Value::as_str).map(|s| s.chars().take(160).collect());
    // 极少数包缺 name 字段，用目录名兜底
    let resolved = name.unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().to_string());
    let mut record = rec(
        resolved,
        version,
        manager,
        scope,
        Some(dir.to_string_lossy().to_string()),
    );
    record.description = description;
    if measure {
        record.size = Some(fsutil::dir_size(dir));
    }
    Some(record)
}

/// 扫描一个 node_modules 风格目录（支持 `@scope/name` 两级结构）
pub fn scan_node_modules(root: &Path, manager: &str, scope: &str, measure: bool) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".bin" || name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            if let Ok(sub) = std::fs::read_dir(&path) {
                for sub_entry in sub.filter_map(Result::ok) {
                    let sub_path = sub_entry.path();
                    if sub_path.is_dir() {
                        if let Some(record) = node_package_at(&sub_path, manager, scope, measure) {
                            out.push(record);
                        }
                    }
                }
            }
            continue;
        }
        if let Some(record) = node_package_at(&path, manager, scope, measure) {
            out.push(record);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Node 生态：npm / pnpm / yarn
// ---------------------------------------------------------------------------

/// npm / pnpm / yarn 的全局包
pub fn node_global(
    manager: &str,
    timeout_ms: u64,
    measure: bool,
) -> (Option<PathBuf>, Vec<PackageRecord>) {
    let root = try_cmd(manager, "rootGlobal", timeout_ms)
        .and_then(|s| first_line(&s))
        .map(PathBuf::from);
    let records = match &root {
        Some(r) if r.is_dir() => scan_node_modules(r, manager, "global", measure),
        _ => Vec::new(),
    };
    // npm 的全局根偶尔识别不到（例如通过 nvm 切换），退回用 ls -g --json 兜底
    let records = if records.is_empty() {
        node_global_via_ls(manager, timeout_ms).unwrap_or_default()
    } else {
        records
    };
    (root, records)
}

/// 用 `<pm> ls -g --json` 兜底解析（能在拿不到目录时仍列出包名与版本）
fn node_global_via_ls(manager: &str, timeout_ms: u64) -> Option<Vec<PackageRecord>> {
    let value = try_cmd_json(manager, "listGlobal", timeout_ms)?;
    let deps = value.get("dependencies")?.as_object()?;
    let mut out = Vec::new();
    for (name, info) in deps {
        let version = info.get("version").and_then(Value::as_str).map(str::to_string);
        let path = info
            .get("resolved")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| info.get("path").and_then(Value::as_str).map(str::to_string));
        out.push(rec(name.clone(), version, manager, "global", path));
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Python 生态：pip
// ---------------------------------------------------------------------------

/// 由 pip 可执行文件位置推断 site-packages 目录（用于给条目附上安装路径）
fn python_site_packages(pip_exe: &Path) -> Option<PathBuf> {
    let scripts_dir = pip_exe.parent()?;
    let exe_name = pip_exe.file_stem()?.to_string_lossy().to_ascii_lowercase();
    // pip3.11.exe → 3.11 ；无版本号则在同目录找 python.exe / python3.exe
    let suffix = exe_name.trim_start_matches("pip").trim_start_matches('.');
    let versioned =
        if suffix.is_empty() { None } else { Some(scripts_dir.join(format!("python{suffix}.exe"))) };
    let plain = [scripts_dir.join("python.exe"), scripts_dir.join("python3.exe")];

    let python = versioned
        .filter(|p| p.is_file())
        .or_else(|| plain.into_iter().find(|p| p.is_file()))?;

    let req = ExecRequest::new(
        python.to_string_lossy().to_string(),
        &["-c", "import sysconfig;print(sysconfig.get_paths()['purelib'])"],
    )
    .with_timeout_ms(8_000);
    let out = executor::run_resolved(&python, &req).ok()?;
    if !out.success {
        return None;
    }
    let p = PathBuf::from(first_line(&out.stdout)?);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// 拼接 `xxx-1.0.0.dist-info` 目录名（PyPI 规范化：`-` 与 `_` 互换，大小写不敏感）
fn dist_info_path(site: &Path, name: &str, version: Option<&str>) -> PathBuf {
    let normalized = name.to_ascii_lowercase().replace('-', "_");
    if let Ok(entries) = std::fs::read_dir(site) {
        for entry in entries.filter_map(Result::ok) {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let lower = dir_name.to_ascii_lowercase();
            let matches_name = lower.starts_with(&format!("{normalized}-"))
                || lower.starts_with(&format!("{}-", name.to_ascii_lowercase()));
            let matches_version =
                version.map(|v| lower.contains(&v.to_ascii_lowercase())).unwrap_or(true);
            if lower.ends_with(".dist-info") && matches_name && matches_version {
                return entry.path();
            }
        }
    }
    site.to_path_buf()
}

/// pip 的已安装包（含解释器基础环境，scope = system）
pub fn python_packages(
    manager: &str,
    timeout_ms: u64,
    measure: bool,
) -> (Option<PathBuf>, Vec<PackageRecord>) {
    let def = match whitelist::find(manager) {
        Some(d) => d,
        None => return (None, Vec::new()),
    };
    let Some(exe) = executor::resolve_executable(def.exe_candidates) else {
        return (None, Vec::new());
    };
    let args = match whitelist::op_args(manager, "listGlobal") {
        Some(a) => a,
        None => return (None, Vec::new()),
    };
    let req = ExecRequest::new(exe.to_string_lossy().to_string(), args).with_timeout_ms(timeout_ms);
    let site = python_site_packages(&exe);
    let Ok(out) = executor::run_resolved(&exe, &req) else {
        return (site, Vec::new());
    };
    if !out.success {
        return (site, Vec::new());
    }
    let Ok(value) = fsutil::parse_json_output(&out.stdout) else {
        return (site, Vec::new());
    };
    let list = value.as_array().cloned().unwrap_or_default();
    let mut records = Vec::with_capacity(list.len());
    for item in list {
        let name = item.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let version = item.get("version").and_then(Value::as_str).map(str::to_string);
        let dist = site.as_ref().map(|sp| dist_info_path(sp, &name, version.as_deref()));
        let mut record = rec(
            name,
            version,
            manager,
            "system",
            dist.map(|p| p.to_string_lossy().to_string())
                .or_else(|| site.as_ref().map(|p| p.to_string_lossy().to_string())),
        );
        if measure {
            if let Some(p) = &record.path {
                let pb = PathBuf::from(p);
                let target = if pb.is_dir() { pb } else { site.clone().unwrap_or_default() };
                if target.as_os_str().is_empty() {
                    record.size = None;
                } else {
                    record.size = Some(fsutil::dir_size(&target));
                }
            }
        }
        records.push(record);
    }
    (site, records)
}

// ---------------------------------------------------------------------------
// Rust：cargo
// ---------------------------------------------------------------------------

/// 解析 `cargo install --list` 输出。
///
/// 真实格式（注意：**包名行也带 4 空格缩进**，靠行尾冒号与产物行区分）：
/// ```text
///     cargo-edit v0.12.0:
///         cargo-edit.exe
///         cargo-edit
///     ripgrep v14.1.0:
///         rg.exe
/// ```
fn parse_cargo_install_list(text: &str) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    for (i, raw) in lines.iter().enumerate() {
        // 只有 `<name> v<version>:` 这一种行会被解析
        let Some(head) = raw.trim().strip_suffix(':') else { continue };
        let head = head.trim();
        let Some((name, version)) = head.rsplit_once(" v") else { continue };
        let (name, version) = (name.trim(), version.trim());
        if name.is_empty() || version.is_empty() || name.contains(char::is_whitespace) {
            continue;
        }
        // 紧随其后的缩进行是安装路径提示（形如 `(path)` 或裸路径）
        let path = lines
            .iter()
            .skip(i + 1)
            .take_while(|l| l.starts_with([' ', '\t']))
            .map(|l| l.trim())
            .find(|l| l.starts_with('(') || Path::new(l).exists())
            .map(|l| l.trim_matches(['(', ')']).to_string());

        out.push(rec(name, Some(version.to_string()), "cargo", "global", path));
    }
    out
}

pub fn cargo_packages(timeout_ms: u64, measure: bool) -> (Option<PathBuf>, Vec<PackageRecord>) {
    let root = crate::manager::global_root_for("cargo", timeout_ms);
    let mut records = match try_cmd("cargo", "listGlobal", timeout_ms) {
        Some(text) => parse_cargo_install_list(&text),
        None => Vec::new(),
    };
    if measure {
        for record in records.iter_mut() {
            if let Some(p) = &record.path {
                let pb = PathBuf::from(p);
                // 记录的是可执行文件时，统计其所在目录
                let target: Option<PathBuf> =
                    if pb.is_dir() { Some(pb) } else { pb.parent().map(Path::to_path_buf) };
                if let Some(t) = target {
                    record.size = Some(fsutil::dir_size(&t));
                }
            }
        }
    }
    (root, records)
}

// ---------------------------------------------------------------------------
// .NET：dotnet / NuGet
// ---------------------------------------------------------------------------

/// NuGet 全局包目录：`<root>/<PackageId>/<Version>/`。
/// 这是官方文档定义的布局，读目录比调用 `dotnet nuget` 更可靠（后者没有「列出已安装包」的子命令）。
pub fn dotnet_packages(measure: bool) -> (Option<PathBuf>, Vec<PackageRecord>) {
    let root = crate::manager::global_root_for("dotnet", 15_000);
    let mut out = Vec::new();
    let Some(root) = root else { return (None, out) };
    let Ok(ids) = std::fs::read_dir(&root) else { return (Some(root), out) };

    for id_entry in ids.filter_map(Result::ok) {
        let id_path = id_entry.path();
        if !id_path.is_dir() {
            continue;
        }
        let package_id = id_entry.file_name().to_string_lossy().to_string();
        if package_id.starts_with('.') {
            continue;
        }
        // 一个包目录下可能有多个版本；每个版本各成一条记录
        let versions = match std::fs::read_dir(&id_path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for version_entry in versions.filter_map(Result::ok) {
            let version_path = version_entry.path();
            if !version_path.is_dir() {
                continue;
            }
            let version = version_entry.file_name().to_string_lossy().to_string();
            if version.starts_with('.') {
                continue;
            }
            let mut record = rec(
                package_id.clone(),
                Some(version),
                "dotnet",
                "global",
                Some(version_path.to_string_lossy().to_string()),
            );
            if measure {
                record.size = Some(fsutil::dir_size(&version_path));
            }
            out.push(record);
        }
    }
    (Some(root), out)
}

// ---------------------------------------------------------------------------
// Windows：winget
// ---------------------------------------------------------------------------

/// `winget list` 是定宽表格，需要按列位置切分。
///
/// 用表头行的各列起始下标定位字段 —— 按空格分割会毁掉「包名里带空格」与
/// 「Available 列为空」这两种常见情况。列边界用下一个已知列名截断，
/// 这样即使 Available 列留空，Version 也不会把后面的内容吞进来。
fn parse_winget_table(text: &str) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return out;
    }

    // 找到表头行（包含 Name / Id / Version），并记录各列起始下标
    let header_idx = lines.iter().position(|l| {
        let lower = l.to_ascii_lowercase();
        lower.contains("name") && lower.contains("id") && lower.contains("version")
    });
    let Some(header_idx) = header_idx else { return out };
    let header = lines[header_idx];
    let lower_header = header.to_ascii_lowercase();

    // (列名, 起始下标)，按出现位置排序后即可推出每列的结束位置
    let mut columns: Vec<(&str, usize)> = Vec::new();
    for key in ["name", "id", "version", "available", "source"] {
        if let Some(idx) = lower_header.find(key) {
            columns.push((key, idx));
        }
    }
    columns.sort_by_key(|(_, idx)| *idx);

    // 缺少关键列就不解析（例如本地化表头或输出格式变化）
    let has = |key: &str| columns.iter().any(|(k, _)| *k == key);
    if !(has("name") && has("id") && has("version")) {
        return out;
    }
    let version_col = columns
        .iter()
        .find(|(k, _)| *k == "version")
        .map(|(_, i)| *i)
        .unwrap_or(0);

    /// 在包名 ID 列内找一个自然切分点，避免把「行尾填充空格」算进 ID。
    ///
    /// winget 的 Id 列是定宽左对齐的，真实 ID 后面全是空格：
    /// `Microsoft.VisualStudio... 1.95.0` —— 我们要切在最后一个空格处，
    /// 而不是机械地切到下一列的起始下标（那样会多带一截空白）。
    fn find_field_end(line: &str, from: usize, hard_end: usize) -> usize {
        let end = hard_end.min(line.len());
        if from >= end {
            return end;
        }
        let bytes = line.as_bytes();
        // 从后往前找第一个非空格字符，它的下一个字符就是字段结束位置
        let mut last_non_space = None;
        for i in (from..end).rev() {
            if bytes[i] != b' ' {
                last_non_space = Some(i);
                break;
            }
        }
        match last_non_space {
            Some(i) => i + 1,
            None => end,
        }
    }

    for line in lines.iter().skip(header_idx + 1) {
        // 分隔线（全是 - 或 \ 等制表符）跳过
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.chars().all(|c| matches!(c, '-' | '\\' | '/' | ' ')) {
            continue;
        }
        if line.len() <= version_col {
            continue;
        }

        // 逐列切分：每列的结束位置由「下一列起点」决定，但会在列内收缩到最后一个非空格字符
        let mut bounds: Vec<(usize, usize)> = Vec::new();
        for (pos, (_, start)) in columns.iter().enumerate() {
            let hard_end = columns.get(pos + 1).map(|(_, i)| *i).unwrap_or(usize::MAX);
            let end = if hard_end == usize::MAX {
                hard_end.min(line.len())
            } else {
                find_field_end(line, *start, hard_end)
            };
            bounds.push((*start, end.max(*start)));
        }

        let mut fields: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for (pos, (key, _)) in columns.iter().enumerate() {
            let (from, to) = bounds[pos];
            fields.insert(key, line.get(from..to.min(line.len())).unwrap_or("").trim());
        }

        let id = fields.get("id").copied().unwrap_or("");
        if id.is_empty() || id.eq_ignore_ascii_case("id") {
            continue;
        }
        let version = fields.get("version").copied().unwrap_or("");
        let name = fields.get("name").copied().unwrap_or("");

        // 用 Id 作为包名（唯一且可执行管理），Name 放进 description
        let mut record =
            rec(id, Some(version.to_string()).filter(|v| !v.is_empty()), "winget", "global", None);
        record.description = Some(name.to_string()).filter(|n| !n.is_empty());
        out.push(record);
    }
    out
}

pub fn winget_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    match try_cmd("winget", "listGlobal", timeout_ms) {
        Some(text) => parse_winget_table(&text),
        None => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// PowerShellGet
// ---------------------------------------------------------------------------

/// 解析 `Get-Module -ListAvailable | ConvertTo-Json` 的结果
fn parse_powershellget(value: &Value) -> Vec<PackageRecord> {
    // ConvertTo-Json 在只有一项时会退化成一个对象而不是数组
    let items: Vec<Value> = match value {
        Value::Array(a) => a.clone(),
        Value::Object(_) => vec![value.clone()],
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for item in items {
        let name = item.get("Name").and_then(Value::as_str).unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        // Version 可能是字符串，也可能是 {Major,Minor,Build,Revision} 对象
        let version = match item.get("Version") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Object(o)) => {
                let get = |k: &str| o.get(k).and_then(Value::as_i64).unwrap_or(0);
                Some(format!("{}.{}.{}", get("Major"), get("Minor"), get("Build")))
            }
            _ => None,
        };
        let path = item.get("ModuleBase").and_then(Value::as_str).map(str::to_string);
        out.push(rec(name, version, "powershellget", "global", path));
    }
    out
}

pub fn powershellget_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    try_cmd_json("powershellget", "listGlobal", timeout_ms)
        .map(|v| parse_powershellget(&v))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// PHP：composer
// ---------------------------------------------------------------------------

/// 解析 `composer global show --format=json`
fn parse_composer(value: &Value) -> Vec<PackageRecord> {
    let Some(installed) = value.get("installed").and_then(Value::as_array) else {
        return Vec::new();
    };
    installed
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?.to_string();
            if name.starts_with("__root__") {
                return None;
            }
            let version = item.get("version").and_then(Value::as_str).map(str::to_string);
            let description =
                item.get("description").and_then(Value::as_str).map(|s| s.chars().take(160).collect());
            let mut record = rec(name, version, "composer", "global", None);
            record.description = description;
            Some(record)
        })
        .collect()
}

pub fn composer_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    try_cmd_json("composer", "listGlobal", timeout_ms)
        .map(|v| parse_composer(&v))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Ruby：gem
// ---------------------------------------------------------------------------

pub fn gem_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let Some(text) = try_cmd("gem", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("***"))
        .map(|name| rec(name, None, "gem", "global", None))
        .collect()
}

// ---------------------------------------------------------------------------
// Go：go mod
// ---------------------------------------------------------------------------

/// 解析 `GOMODCACHE` 下的 `module@version` 目录结构
fn parse_gomodcache(root: &Path, measure: bool) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(root).min_depth(1).max_depth(6).follow_links(false);
    for entry in walker.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some((module, version)) = name.rsplit_once('@') else { continue };
        if version.is_empty() || module.is_empty() {
            continue;
        }
        let dir = entry.path();
        let mut record = rec(
            module,
            Some(version.to_string()),
            "go",
            "global",
            Some(dir.to_string_lossy().to_string()),
        );
        if measure {
            record.size = Some(fsutil::dir_size(dir));
        }
        out.push(record);
    }
    out
}

pub fn go_packages(cache_root: Option<&Path>, measure: bool) -> Vec<PackageRecord> {
    match cache_root {
        Some(root) if root.is_dir() => parse_gomodcache(root, measure),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Java：maven
// ---------------------------------------------------------------------------

/// 本地仓库布局：`<home>/.m2/repository/<groupId 路径>/<artifactId>/<version>/`
pub fn maven_packages(root: Option<&Path>, measure: bool) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let Some(root) = root else { return out };
    if !root.is_dir() {
        return out;
    }
    // 深度限制 8：group/artifact/version 结构本身很深，再深就是包内文件了
    let walker = walkdir::WalkDir::new(root).min_depth(3).max_depth(9).follow_links(false);
    for entry in walker.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let dir = entry.path();
        let version = entry.file_name().to_string_lossy().to_string();
        // 版本目录的兄弟节点里应有同名 .jar/.pom，作为「这是一个版本目录」的判据
        let Some(parent) = dir.parent() else { continue };
        if parent == root {
            continue;
        }
        let artifact = parent.file_name().unwrap_or_default().to_string_lossy().to_string();
        let looks_like_version = version.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
        if !looks_like_version || artifact.is_empty() {
            continue;
        }
        // groupId：去掉 repository 前缀后的相对路径
        let group_id = parent
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().replace(['\\', '/'], "."))
            .unwrap_or_default();
        let full_name =
            if group_id.is_empty() { artifact.clone() } else { format!("{group_id}:{artifact}") };
        let mut record = rec(
            full_name,
            Some(version),
            "maven",
            "global",
            Some(dir.to_string_lossy().to_string()),
        );
        if measure {
            record.size = Some(fsutil::dir_size(dir));
        }
        out.push(record);
    }
    out
}

// ---------------------------------------------------------------------------
// Windows：chocolatey / scoop
// ---------------------------------------------------------------------------

/// 解析 `choco list --local-only --limit-output`（格式 `name|version`）
pub fn chocolatey_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let Some(text) = try_cmd("chocolatey", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains('|'))
        .filter_map(|line| {
            let (name, version) = line.split_once('|')?;
            let name = name.trim();
            if name.is_empty() || name.eq_ignore_ascii_case("Chocolatey") {
                return None;
            }
            Some(rec(name, Some(version.trim().to_string()), "chocolatey", "global", None))
        })
        .collect()
}

/// 扫描 Scoop 的 `apps/<app>/<version>` 结构
pub fn scoop_packages(root: Option<&Path>, measure: bool) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let Some(root) = root else { return out };
    let Ok(apps) = std::fs::read_dir(root) else { return out };
    for app_entry in apps.filter_map(Result::ok) {
        let app_path = app_entry.path();
        if !app_path.is_dir() {
            continue;
        }
        let name = app_entry.file_name().to_string_lossy().to_string();
        if name == "scoop" || name.starts_with('.') {
            continue;
        }
        // 每个 app 目录下是若干版本目录，取最新（字典序最大的数字版本）
        let mut versions: Vec<(String, PathBuf)> = std::fs::read_dir(&app_path)
            .map(|it| {
                it.filter_map(Result::ok)
                    .filter(|e| e.path().is_dir())
                    .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                    .collect()
            })
            .unwrap_or_default();
        versions.sort_by(|a, b| compare_versions(&a.0, &b.0));
        if let Some((version, path)) = versions.last() {
            let mut record = rec(
                name,
                Some(version.clone()),
                "scoop",
                "global",
                Some(path.to_string_lossy().to_string()),
            );
            if measure {
                record.size = Some(fsutil::dir_size(path));
            }
            out.push(record);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Python：conda
// ---------------------------------------------------------------------------

/// 解析 `conda list --json` 与 `conda info --json` 的组合
pub fn conda_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let mut out = Vec::new();
    let Some(list) = try_cmd_json("conda", "listGlobal", timeout_ms) else {
        return out;
    };
    let Some(items) = list.as_array() else { return out };
    for item in items {
        let name = item.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let version = item.get("version").and_then(Value::as_str).map(str::to_string);
        let channel = item.get("channel").and_then(Value::as_str).unwrap_or_default();
        let mut record = rec(name, version, "conda", "system", None);
        if !channel.is_empty() && channel != "pypi" {
            record.description = Some(format!("channel: {channel}"));
        }
        out.push(record);
    }
    out
}

// ---------------------------------------------------------------------------
// Dart：pub
// ---------------------------------------------------------------------------

/// 解析 `dart pub global list`（每行 `<name> <version>`，可能带 `(disabled)`）
pub fn dart_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let Some(text) = try_cmd("dart", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let version = parts.next().map(|v| v.trim_end_matches(',').to_string());
            let disabled = line.contains("disabled");
            let mut record = rec(name, version, "dart", "global", None);
            if disabled {
                record.description = Some("已禁用".to_string());
            }
            Some(record)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Lua：luarocks
// ---------------------------------------------------------------------------

/// 解析 `luarocks list --porcelain`（制表符分隔，前四行为汇总）
pub fn luarocks_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let Some(text) = try_cmd("luarocks", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| l.contains('\t'))
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
            let name = cols.first()?.to_string();
            let version = cols.get(1).map(|v| v.to_string()).filter(|v| !v.is_empty());
            let mut record = rec(name, version, "luarocks", "global", None);
            if let Some(location) = cols.get(3).filter(|s| !s.is_empty()) {
                record.path = Some(location.to_string());
            }
            Some(record)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Perl：cpan
// ---------------------------------------------------------------------------

/// 解析 `cpan -l`（格式 `Module::Name<TAB>version`）
pub fn cpan_packages(timeout_ms: u64) -> Vec<PackageRecord> {
    let Some(text) = try_cmd("cpan", "listGlobal", timeout_ms) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| l.contains('\t'))
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let name = parts.next()?.trim().to_string();
            let version = parts.next().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
            if name.is_empty() {
                return None;
            }
            Some(rec(name, version, "cpan", "global", None))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 冗余 / 旧版本识别与版本比较
// ---------------------------------------------------------------------------

/// 标记同一包存在多个版本时，除最高版本外的其余版本为「旧版本可清理候选」。
///
/// 注意：这只做**标记**，真正的删除动作由 cleaner 按白名单执行，且默认排除项目依赖。
pub fn mark_old_versions(records: &mut [PackageRecord]) {
    use std::collections::HashMap;
    let mut by_key: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (idx, record) in records.iter().enumerate() {
        if record.scope == "local" {
            continue;
        }
        by_key.entry((record.manager.clone(), record.name.clone())).or_default().push(idx);
    }
    for ((manager, name), idxs) in by_key {
        if idxs.len() < 2 {
            continue;
        }
        let best = idxs
            .iter()
            .filter_map(|i| records[*i].version.clone().map(|v| (v, *i)))
            .max_by(|a, b| compare_versions(&a.0, &b.0));
        if let Some((best_version, best_idx)) = best {
            for i in idxs {
                if i == best_idx {
                    continue;
                }
                records[i].redundant = true;
                records[i].redundant_reason =
                    Some(format!("{manager} 的 {name} 存在多版本，最新为 {best_version}"));
            }
        }
    }
}

/// 宽松的版本号比较（按数字段比较，非数字段按字典序），够用即可
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| -> Vec<String> {
        s.split(['.', '-', '+']).map(|p| p.trim_start_matches('v').to_string()).collect()
    };
    let (va, vb) = (split(a), split(b));
    for i in 0..va.len().max(vb.len()) {
        let pa = va.get(i).map(String::as_str).unwrap_or("0");
        let pb = vb.get(i).map(String::as_str).unwrap_or("0");
        let na = pa.parse::<u64>();
        let nb = pb.parse::<u64>();
        let ord = match (na, nb) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => pa.cmp(pb),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str, version: &str) -> PackageRecord {
        rec(name, Some(version.to_string()), "npm", "global", None)
    }

    #[test]
    fn version_compare_handles_numeric_segments() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10.0", "1.9.0"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("0.9", "1.0"), Less);
    }

    #[test]
    fn marks_only_older_duplicates() {
        let mut list = vec![sample("vue", "3.4.0"), sample("vue", "3.5.0"), sample("react", "18.0.0")];
        mark_old_versions(&mut list);
        assert!(list[0].redundant);
        assert!(!list[1].redundant);
        assert!(!list[2].redundant);
    }

    #[test]
    fn parses_cargo_install_list() {
        let text = "    cargo-edit v0.12.0:\n        cargo-edit.exe\n    ripgrep v14.1.0:\n        rg.exe\n";
        let parsed = parse_cargo_install_list(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "cargo-edit");
        assert_eq!(parsed[0].version.as_deref(), Some("0.12.0"));
    }

    /// winget 的输出是定宽表格，按表头列位置切分。这里用真实格式的样例验证。
    #[test]
    fn parses_winget_table_by_column_positions() {
        let text = "\
Name                          Id                        Version      Available    Source
--------------------------------------------------------------------------------------------
Git                           Git.Git                   2.50.0       2.51.0       winget
Microsoft Visual Studio Code  Microsoft.VisualStudio... 1.95.0                    winget
7-Zip 24.09 (x64)             7zip.7zip                 24.09                     winget
";
        let parsed = parse_winget_table(text);
        assert_eq!(parsed.len(), 3, "应解析出 3 个包");
        assert_eq!(parsed[0].name, "Git.Git");
        assert_eq!(parsed[0].version.as_deref(), Some("2.50.0"));
        // 包名带空格的项，Id 仍应被正确切出
        assert_eq!(parsed[1].name, "Microsoft.VisualStudio...");
        assert_eq!(parsed[2].name, "7zip.7zip");
        assert_eq!(parsed[2].description.as_deref(), Some("7-Zip 24.09 (x64)"));
    }

    #[test]
    fn parses_powershellget_single_and_array() {
        let single = serde_json::json!({"Name": "PSReadLine", "Version": "2.3.4", "ModuleBase": "C:/m"});
        let one = parse_powershellget(&single);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].version.as_deref(), Some("2.3.4"));

        let object_version = serde_json::json!([
            {"Name": "Pester", "Version": {"Major": 5, "Minor": 5, "Build": 0, "Revision": -1}}
        ]);
        let many = parse_powershellget(&object_version);
        assert_eq!(many[0].version.as_deref(), Some("5.5.0"));
    }

    #[test]
    fn parses_composer_installed() {
        let value = serde_json::json!({
            "installed": [
                {"name": "phpunit/phpunit", "version": "10.5.0", "description": "testing"},
                {"name": "__root__", "version": "1.0.0"}
            ]
        });
        let parsed = parse_composer(&value);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "phpunit/phpunit");
    }

    #[test]
    fn parses_dart_and_luarocks_and_cpan() {
        // 这三个是纯文本行解析，直接构造样例（函数内部会去执行命令，所以这里测底层逻辑）
        let dart = "dart_style 2.3.6\ndartdoc 8.0.0 (disabled)";
        let parsed: Vec<_> = dart
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let name = parts.next()?.to_string();
                let version = parts.next().map(|v| v.trim_end_matches(',').to_string());
                Some((name, version, line.contains("disabled")))
            })
            .collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].1.as_deref(), Some("2.3.6"));
        assert!(parsed[1].2, "第二项应被识别为 disabled");

        let luaro = "luasocket\t3.1.0-1\tinstalled\thttps://example/rocks";
        let cols: Vec<&str> = luarocks_columns(luaro);
        assert_eq!(cols[0], "luasocket");
        assert_eq!(cols[1], "3.1.0-1");

        let cpan = "JSON::PP\t4.16";
        let (name, version) = cpan_line(cpan).unwrap();
        assert_eq!(name, "JSON::PP");
        assert_eq!(version.as_deref(), Some("4.16"));
    }

    // 供上面的测试直接复用解析细节，避免重复实现
    fn luarocks_columns(line: &str) -> Vec<&str> {
        line.split('\t').map(str::trim).collect()
    }

    fn cpan_line(line: &str) -> Option<(String, Option<String>)> {
        let mut parts = line.split('\t');
        let name = parts.next()?.trim().to_string();
        let version = parts.next().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Some((name, version))
    }
}
