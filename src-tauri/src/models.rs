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
    /// 所属语言生态：node / python / rust / dotnet / windows ...
    pub language: String,
    /// 优先级阶段：1 = 一期可用，2/3/4 = 后续阶段（界面弱化展示）
    pub tier: u8,
    /// 该管理器适用的平台说明，例如「仅 Linux」「macOS / Linux」
    pub platforms: String,
    /// 是否适用于**当前**操作系统。false = 这东西本机不可能有，
    /// 界面要说明原因，而不是提示「未在 PATH 中找到」。
    pub platform_applicable: bool,
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
    /// 包管理器品牌 logo（内联 SVG data URI）。按探测时的主题生成，
    /// 切换主题后可调用 `manager_logos` 重新获取。
    pub logo: Option<String>,
    /// 未安装时的下载入口
    pub download_url: Option<String>,
    pub docs_url: Option<String>,
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

/// 包内的「子节点」：插件、扩展、资源或运行时依赖。
///
/// 设计上刻意做成通用树节点，这样 npm 的插件、pip 的依赖、VS Code 扩展目录、
/// NuGet 的包内容都能用同一套 UI 渲染，不必为每个生态单独建模。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginNode {
    /// 节点类型：plugin | dependency | file | dir | runtime
    pub node_type: String,
    pub name: String,
    pub version: Option<String>,
    /// 安装路径（允许为空：部分来源无法定位）
    pub path: Option<String>,
    pub size: Option<u64>,
    /// 补充说明，例如 "由 package.json 的 contributes 声明"
    pub note: Option<String>,
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
    /// 最新版本（来自仓库元数据，未查询时为 null）
    pub latest_version: Option<String>,
    /// 包内子节点（插件 / 扩展 / 依赖）。按需加载，未加载时为空数组。
    #[serde(default)]
    pub plugins: Vec<PluginNode>,
    /// 子节点是否已加载过 —— 用于区分「没有插件」与「还没查」
    #[serde(default)]
    pub plugins_loaded: bool,

}

/// 包管理动作（右键菜单项）。
///
/// 重要：本工具**不替用户执行卸载/更新**。这里只把「该用什么命令」结构化地告诉界面，
/// 一期这些按钮为占位状态，避免误操作破坏用户环境。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementAction {
    /// manage | update | uninstall | install | disable | openDocs | inspect
    pub action: String,
    pub label: String,
    /// 是否需要联网
    pub online: bool,
    /// 是否具有破坏性（界面用红色标注）
    pub destructive: bool,
    /// 一期是否真正可用；false = 占位按钮
    pub enabled: bool,
    /// 等价命令（仅供参考展示，本工具不会执行）
    pub command_hint: Option<String>,
    /// 不可用原因
    pub note: Option<String>,
}

/// 未安装管理器的引导信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallHint {
    pub manager_id: String,
    pub name: String,
    pub language: String,
    pub download_url: Option<String>,
    pub docs_url: Option<String>,
    /// 推荐安装方式（按平台给出）
    pub install_hint: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 风险等级：safe 仅缓存 / warn 需注意 / protected 禁止删除
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 打开外部链接的请求。
///
/// 安全设计：**不接受任意 URL**。前端只能提交「已知管理器 id」或「在允许列表内的 URL」，
/// 由 Rust 侧解析成最终地址，避免被注入钓鱼链接。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenLinkRequest {
    /// manager | docs | url
    pub kind: String,
    /// kind = manager 时为管理器 id；kind = url 时为完整地址
    pub target: String,
}

/// 包内子节点查询请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginsRequest {
    pub manager: String,
    /// 已通过 `validate::package_name` 校验的包名
    pub package: String,
    pub version: Option<String>,
    /// 安装路径（必须落在该管理器的全局根之内，由 Rust 侧复核）
    pub path: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// 应用设置（语言 / 主题）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// zh-CN | en-US
    pub language: String,
    /// dark | light | system
    pub theme: String,
    /// 是否在启动时自动扫描
    pub scan_on_startup: bool,
    /// 是否列出「当前系统不适用」的包管理器（如 Windows 上的 apt / brew）。
    /// 默认关闭：这些管理器永远检测不到，列出来只会干扰阅读。
    pub show_other_platforms: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            theme: "dark".to_string(),
            scan_on_startup: true,
            show_other_platforms: false,
        }
    }
}

/// 一次真实包管理操作（更新 / 卸载 / 安装）的结果。
///
/// 带完整输出与等价命令，便于用户在出问题时自行复核或重跑。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageOpResult {
    pub manager_id: String,
    pub package: String,
    /// update | uninstall | install
    pub action: String,
    /// 等价命令（用于展示与日志，不用于执行）
    pub command: String,
    pub success: bool,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub message: Option<String>,
    pub duration_ms: u64,
    /// 执行日志的落盘位置（可见命令行窗口里同样能看到完整输出）
    pub log_path: Option<String>,
}

/// 单个管理器的扫描结果。
///
/// 存在的意义：把「一次全量扫描」拆成「每个管理器一次」，
/// 前端可以在每个管理器扫完后立刻渲染，而不是等最慢的那个 ——
/// winget 要几秒、pip 要几秒，串起来用户会盯着空白页干等。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerScanResult {
    pub manager_id: String,
    pub packages: Vec<PackageRecord>,
    pub cache: Option<CacheStats>,
    pub duration_ms: u64,
    /// 该管理器是否被成功读取（false 时 packages 为空且 reason 有值）
    pub ok: bool,
    pub reason: Option<String>,
}

/// 图标请求：返回 data URI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconResponse {
    pub key: String,
    pub data_uri: String,
    /// 是否来自缓存
    pub cached: bool,
}

/// 包管理器的品牌 logo 查询（按主题批量取，供侧边栏/详情页用）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoRequest {
    /// dark | light
    pub theme: String,
    /// 为空表示全部管理器
    #[serde(default)]
    pub managers: Vec<String>,
}

/// 某个管理器的 logo
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoResponse {
    pub manager_id: String,
    pub data_uri: String,
}

/// 在包仓库里搜索可安装的新包
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseRequest {
    pub manager: String,
    /// 搜索关键词（会先过 `validate::search_query`）
    pub query: String,
    #[serde(default = "default_browse_limit")]
    pub limit: usize,
    pub timeout_ms: Option<u64>,
}

fn default_browse_limit() -> usize {
    25
}

/// 仓库里的一个可安装包
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePackage {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// 下载量 / 热度（有则展示）
    pub downloads: Option<u64>,
    /// 包主页地址
    pub homepage: Option<String>,
    /// 等价安装命令（本工具不代为执行）
    pub install_command: Option<String>,
}

/// 在线浏览的结果。
///
/// 之所以不直接返回 `Vec<RemotePackage>`：空数组无法区分三种完全不同的情况 ——
/// 「查了、确实没有这个包」「网络失败 / 被限流」「该生态不支持关键词搜索」。
/// 让后端把差异显式带回来，界面才能给出有用的提示，
/// 而不是一律显示「没有找到匹配的包」（这是之前最误导用户的地方）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseResult {
    pub packages: Vec<RemotePackage>,
    /// 是否真的发起了查询（false = 该生态不支持关键词搜索）
    pub attempted: bool,
    /// 是否发生了网络 / 解析失败
    pub failed: bool,
    /// 失败或限制的具体说明
    pub note: Option<String>,
    /// 使用提示，例如「PyPI 不支持关键词搜索，已按精确名查询」
    pub hint: Option<String>,
}

/// 安装动作的结果：只返回「该执行什么命令」，**不会真的执行**
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub manager_id: String,
    pub package: String,
    /// 可复制的安装命令
    pub command: String,
    /// 是否需要联网
    pub online: bool,
    /// 是否需要管理员权限
    pub requires_admin: bool,
    /// 为什么不由本工具执行
    pub explanation: String,
}
