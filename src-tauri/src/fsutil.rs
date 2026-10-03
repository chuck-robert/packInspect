//! 文件系统工具：目录体积统计、文本/JSON 读取、人类可读体积格式化。

use crate::error::{AppError, AppResult};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// 单次扫描的条目上限，防止误选巨大目录（如整个 D 盘）导致界面卡死
pub const MAX_SCAN_ENTRIES: u64 = 400_000;

#[derive(Debug, Clone, Default)]
pub struct DirStat {
    pub bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    /// 最近修改时间
    pub last_modified: Option<SystemTime>,
    /// 是否触达条目上限被截断
    pub truncated: bool,
}

/// 统计目录体积。
///
/// - `max_entries` 命中即停（`truncated = true`），保证 UI 响应性
/// - 默认不跟随符号链接，避免 Windows junction 造成无限递归
pub fn dir_stat(root: &Path, max_entries: u64) -> DirStat {
    let mut stat = DirStat::default();
    if !root.is_dir() {
        if root.is_file() {
            if let Ok(md) = root.metadata() {
                stat.bytes = md.len();
                stat.file_count = 1;
                stat.last_modified = md.modified().ok();
            }
        }
        return stat;
    }

    let mut visited = 0u64;
    for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(Result::ok) {
        visited += 1;
        if visited > max_entries {
            stat.truncated = true;
            break;
        }
        let md = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.is_dir() {
            stat.dir_count += 1;
            continue;
        }
        stat.file_count += 1;
        stat.bytes += md.len();
        if let Ok(mtime) = md.modified() {
            if stat.last_modified.map(|cur| mtime > cur).unwrap_or(true) {
                stat.last_modified = Some(mtime);
            }
        }
    }
    stat
}

/// 只统计体积，忽略上限截断细节
pub fn dir_size(root: &Path) -> u64 {
    dir_stat(root, MAX_SCAN_ENTRIES).bytes
}

/// 列出目录的一级子项，并统计各自体积（用于缓存目录树）
pub fn children_stat(root: &Path, max_entries: u64) -> Vec<(String, PathBuf, DirStat)> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return result,
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let stat = dir_stat(&path, max_entries);
        result.push((name, path, stat));
    }
    // 体积从大到小，界面默认按占用排序
    result.sort_by(|a, b| b.2.bytes.cmp(&a.2.bytes));
    result
}

/// 读取文本文件。超过 `max_bytes` 时截断，避免读取巨型文件卡住内存。
pub fn read_text(path: &Path, max_bytes: u64) -> AppResult<String> {
    let file = std::fs::File::open(path)
        .map_err(|e| AppError::io(format!("打开 {} 失败: {e}", path.display())))?;
    let mut buf = Vec::new();
    file.take(max_bytes).read_to_end(&mut buf)?;
    // 配置文件基本是 UTF-8；个别老配置可能是 GBK，用 lossy 降级保证不 panic
    Ok(String::from_utf8_lossy(&buf).to_string())
}

/// 读取并解析 JSON 文件；文件不存在返回 `Ok(None)`
pub fn read_json(path: &Path, max_bytes: u64) -> AppResult<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_text(path, max_bytes)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::new("PARSE_ERROR", format!("解析 {} 失败: {e}", path.display())))?;
    Ok(Some(value))
}

/// 解析命令输出里的 JSON（有些工具会在 JSON 前打印提示行）
pub fn parse_json_output(raw: &str) -> AppResult<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::new("EMPTY_OUTPUT", "命令没有返回内容"));
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    // 退一步：截取第一个 '{' 或 '[' 到最后一个匹配括号
    let start = trimmed.find(['{', '[']);
    if let Some(start) = start {
        let open = trimmed.as_bytes()[start] as char;
        let close = if open == '{' { '}' } else { ']' };
        if let Some(end) = trimmed.rfind(close) {
            if end > start {
                if let Ok(v) = serde_json::from_str::<Value>(&trimmed[start..=end]) {
                    return Ok(v);
                }
            }
        }
    }
    Err(AppError::new("PARSE_ERROR", "命令输出不是合法 JSON"))
}

/// 人类可读体积，与前端 `formatBytes` 保持一致
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[idx])
    }
}

/// 写入文件（自动创建父目录），用于导出报告与配置备份
pub fn write_text(path: &Path, content: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
        .map_err(|e| AppError::io(format!("写入 {} 失败: {e}", path.display())))
}

/// 带时间戳的备份文件名，例如 `pip.ini.bak-20240501-101530`
pub fn backup_suffix() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.00 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.00 MB");
    }

    #[test]
    fn parse_json_tolerates_prefix_noise() {
        let v = parse_json_output("npm warn something\n{\"a\":1}").unwrap();
        assert_eq!(v["a"], 1);
        assert!(parse_json_output("not json").is_err());
    }
}
