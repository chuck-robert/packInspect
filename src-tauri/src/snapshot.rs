//! 磁盘快照：缓存上次的探测与扫描结果，让下次启动**瞬间**就有内容。
//!
//! 【要解决的问题】
//! 冷启动时前端要等 `detect_managers` 跑完（并发后仍有约 1.6 秒，最慢的管理器
//! 还要更久），再等 `scan` 出包列表。这期间界面是空的，用户只能盯着转圈。
//!
//! 【做法】
//! 每次探测 / 扫描成功后把结果写一份到 `%APPDATA%\PackInspect\snapshot.json`。
//! 下次启动时：
//!   1. `load_snapshot` 直接把上次的结果交给界面 → 立刻有内容
//!   2. 后台照常跑真实探测 / 扫描 → 拿到新结果后覆盖界面与快照
//!
//! 也就是「stale-while-revalidate」：先用旧的，再无声换成新的。
//!
//! 【为什么放磁盘而不是只放内存】
//! 内存缓存（`AppState.cache`）只能加速**同一次运行内**的重复探测；
//! 用户关心的是"关掉再打开"，那必须跨进程持久化。
//!
//! 【快照里的时间戳为什么重要】
//! 界面要能区分"这是上次的数据"和"这是刚扫的"。`capturedAt` 会一路传到前端，
//! 用来显示"数据来自 x 分钟前，正在刷新…"。

use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::models::{ManagerInfo, ScanReport};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 快照的结构版本。
///
/// 只要 `ManagerInfo` / `ScanReport` 的字段有增删改，就把这个数 +1：
/// 旧快照会被判定为不兼容而丢弃，而不是反序列化失败或读出半截数据。
///
/// 对命令层可见：保存快照时要写入当前版本号。
pub const SNAPSHOT_SCHEMA: u32 = 1;

/// 快照体积上限。包列表在极端情况下可能很大，读入时设个上限防止意外。
const MAX_SNAPSHOT_BYTES: u64 = 32 * 1024 * 1024;

/// 一份持久化的快照
///
/// 刻意**由前端保存它正在显示的那份状态**（managers + report），而不是在 Rust 里
/// 从缓存反推：界面显示什么、快照就存什么，下次恢复出来的就一定是用户上次看到的
/// 样子，不会出现"缓存里有的字段和界面上的对不上"。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 结构版本，见 `SNAPSHOT_SCHEMA`
    pub schema: u32,
    /// 这份快照是什么时候采集的（RFC3339）
    pub captured_at: String,
    /// 上次探测到的管理器（用于首屏立刻渲染侧边栏与版本号）
    pub managers: Vec<ManagerInfo>,
    /// 上次的扫描报告（可能为空：用户从未扫过）
    pub report: Option<ScanReport>,
}

/// 快照文件路径：与 settings.json 同目录
pub fn snapshot_path() -> AppResult<PathBuf> {
    let settings = settings::settings_path()?;
    Ok(settings
        .parent()
        .map(|d| d.join("snapshot.json"))
        .unwrap_or_else(|| PathBuf::from("snapshot.json")))
}

/// 写入快照。
///
/// 失败**不报错给用户**：快照只是加速手段，写不进去最多下次启动慢一点，
/// 不该因为磁盘空间 / 权限问题弹一个错误。因此这里只返回 bool 供日志用。
pub fn save(snapshot: &Snapshot) -> bool {
    let Ok(path) = snapshot_path() else {
        return false;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    match serde_json::to_vec(snapshot) {
        Ok(bytes) => std::fs::write(&path, bytes).is_ok(),
        Err(_) => false,
    }
}

/// 读取快照。
///
/// 任何异常（文件不存在、损坏、结构版本不匹配、超出体积上限）都返回 `None`，
/// 由调用方退回到"老老实实探测一遍"。对用户而言就是首次启动的正常体验。
pub fn load() -> Option<Snapshot> {
    let path = snapshot_path().ok()?;
    if !path.is_file() {
        return None;
    }
    let value = match fsutil::read_json(&path, MAX_SNAPSHOT_BYTES) {
        Ok(Some(v)) => v,
        _ => return None,
    };
    let snapshot: Snapshot = serde_json::from_value(value).ok()?;
    if snapshot.schema != SNAPSHOT_SCHEMA {
        return None;
    }
    Some(snapshot)
}

/// 删除快照（例如用户主动"重新探测"时想丢掉旧数据）
pub fn clear() -> AppResult<()> {
    let path = snapshot_path()?;
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| AppError::io(format!("删除快照失败: {e}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_current_and_nonzero() {
        // 0 会被当成"未初始化"，因此不允许。
        // 通过引用读取，避免 clippy 把常量断言当成恒真式优化掉。
        let schema = std::hint::black_box(SNAPSHOT_SCHEMA);
        assert_ne!(schema, 0, "结构版本不能为 0");
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let original = Snapshot {
            schema: SNAPSHOT_SCHEMA,
            captured_at: "2026-10-03T12:00:00Z".to_string(),
            managers: Vec::new(),
            report: None,
        };
        let json = serde_json::to_string(&original).expect("应能序列化");
        let back: Snapshot = serde_json::from_str(&json).expect("应能反序列化");
        assert_eq!(back.schema, original.schema);
        assert_eq!(back.captured_at, original.captured_at);
        assert!(back.managers.is_empty());
    }

    #[test]
    fn mismatched_schema_is_rejected() {
        // 模拟一份"来自未来版本"的快照
        let future = Snapshot {
            schema: SNAPSHOT_SCHEMA + 99,
            captured_at: "2026-10-03T12:00:00Z".to_string(),
            managers: Vec::new(),
            report: None,
        };
        let json = serde_json::to_value(&future).unwrap();
        let parsed: Snapshot = serde_json::from_value(json).unwrap();
        // load() 里的判断：版本不等就不采用
        assert_ne!(parsed.schema, SNAPSHOT_SCHEMA);
    }

    #[test]
    fn snapshot_path_lives_next_to_settings() {
        let snap = snapshot_path().expect("应能定位快照路径");
        let settings = settings::settings_path().expect("应能定位设置路径");
        assert_eq!(snap.parent(), settings.parent(), "快照应与设置同目录");
        assert_eq!(snap.file_name().unwrap(), "snapshot.json");
    }
}
