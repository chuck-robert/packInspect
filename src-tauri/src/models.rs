//! 后端 ↔ 前端共享数据结构。
//!
//! 约定：Rust 侧全部 `rename_all = "camelCase"`，与 `src/types/index.ts` 一一对应，
//! 任何字段改动必须同步修改 TS 定义，否则前端类型检查会失败（这是刻意的，用来防漂移）。

use serde::{Deserialize, Serialize};

/// 包管理器探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerInfo {
    /// 稳定标识符，如 "npm" / "pip"
    pub id: String,
    /// 展示名，如 "npm"
    pub name: String,
    /// 所属语言生态：node / python / rust / go ...
    pub language: String,
    /// 是否在本机检测到可执行文件
    pub detected: bool,
    /// `--version` 输出（已清理首行）
    pub version: Option<String>,
    /// 解析到的可执行文件绝对路径
    pub exe_path: Option<String>,
    /// 全局包安装根目录
    pub global_root: Option<String>,
    /// 缓存目录
    pub cache_dir: Option<String>,
    /// 读取到的配置文件路径（npmrc / pip.conf 等）
    pub config_file: Option<String>,
    /// 当前镜像源配置
    pub registry: Option<RegistryConfig>,
    /// 探测过程中的非致命警告（例如版本命令超时）
    pub warnings: Vec<String>,
}

/// 镜像源 / 配置文件现状
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryConfig {
    pub manager_id: String,
    /// 源类型 key，如 "registry"（npm）、"index-url"（pip）
    pub entries: Vec<RegistryEntry>,
    /// 原始配置文件内容（用于「查看原文」与备份对比）
    pub raw: String,
    /// 配置文件是否可写（决定前端是否允许保存）
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub key: String,
    pub value: String,
    /// 该值是否来自用户级配置（而非内置默认）
    pub user_defined: bool,
    /// 可选说明，如 "官方源" / "淘宝镜像"
    pub hint: Option<String>,
}

/// 一个已安装的包
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRecord {
    pub name: String,
    pub version: Option<String>,
    /// 来源包管理器 id
    pub manager: String,
    /// global | local | system
    pub scope: String,
    /// 安装路径（目录或 dist-info 路径）
    pub path: Option<String>,
    /// 该包占用字节数（目录扫描，可能为 null 表示未统计）
    pub size: Option<u64>,
    /// 是否被识别为可清理候选（冗余 / 旧版本）
    pub redundant: bool,
    /// 冗余原因说明
    pub redundant_reason: Option<String>,
    pub description: Option<String>,
    /// 最新版本（来自仓库元数据，暂未实现则为 null）
    pub latest_version: Option<String>,
}

/// 缓存目录统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub manager_id: String,
    pub path: String,
    pub exists: bool,
    pub total_bytes: u64,
    pub file_count: u64,
    /// 最近修改时间（RFC3339）
    pub last_modified: Option<String>,
    /// 按一级子目录聚合的占用
    pub children: Vec<CacheChild>,
    /// 扫描是否因目录过大而提前结束
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheChild {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
}

/// 清理候选（一条 = 一个可删除的路径）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanCandidate {
    /// 稳定 id：`{manager}:{kind}:{hash}`
    pub id: String,
    pub manager_id: String,
    pub kind: CleanKind,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub reason: String,
    /// 风险等级：safe 仅缓存 / warn 需注意
    pub risk: String,
    /// 是否被安全策略硬性禁止删除（前端只能展示，不能勾选）
    pub protected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanKind {
    /// 包管理器自身的缓存/下载目录
    Cache,
    /// 同一包的历史旧版本
    OldVersion,
    /// 临时文件（*.tmp / 中断下载残留）
    Temp,
    /// 孤立文件（无索引引用的缓存条目）
    Orphan,
}

/// 清理执行结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub candidate_id: String,
    pub path: String,
    pub ok: bool,
    pub freed_bytes: u64,
    pub message: Option<String>,
}

/// 一次完整扫描的报告。
///
/// `Deserialize` 是必需的：`export_report` 命令需要把前端持有的报告作为**入参**传回后端，
/// Tauri 的 `CommandArg` 要求入参可反序列化。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    /// 报告生成时间（RFC3339）
    pub generated_at: String,
    pub hostname: Option<String>,
    pub os: String,
    /// 本次扫描涉及的包管理器
    pub managers: Vec<ManagerInfo>,
    pub packages: Vec<PackageRecord>,
    pub caches: Vec<CacheStats>,
    pub total_packages: usize,
    pub total_cache_bytes: u64,
    pub duration_ms: u64,
}

/// 扫描请求参数（前端只传结构化意图，不传命令行）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    /// 要扫描的包管理器 id 列表；空数组 = 所有已探测到的
    #[serde(default)]
    pub managers: Vec<String>,
    /// 是否统计每个包目录的体积（较慢，默认 false）
    #[serde(default)]
    pub measure_package_size: bool,
    /// 单次命令超时毫秒
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    20_000
}

impl Default for ScanRequest {
    fn default() -> Self {
        Self { managers: Vec::new(), measure_package_size: false, timeout_ms: default_timeout() }
    }
}

/// 清理请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanRequest {
    pub candidate_ids: Vec<String>,
    /// true = 只计算将要删除的内容，不真正删除（默认）
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

fn default_true() -> bool {
    true
}

/// 导出请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    /// json | csv | markdown
    pub format: String,
    /// 目标文件绝对路径（由前端另存为对话框提供）
    pub target_path: String,
}
