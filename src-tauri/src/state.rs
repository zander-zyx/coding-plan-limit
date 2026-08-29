//! 运行时共享状态：快照缓存、通知去重、刷新信号、弹窗悬停标记。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64};

use tokio::sync::{Mutex, Notify};

use crate::update::UpdateInfo;
use crate::usage::types::Snapshot;

#[derive(Default)]
pub struct AppState {
    /// plan_id → 最新快照
    pub snapshots: Mutex<HashMap<String, Snapshot>>,
    /// 通知去重：plan_id → 告警记录（键 / 最近提醒时间 / 同键累计刷新次数）
    pub notified: Mutex<HashMap<String, NotifyRecord>>,
    /// 立即刷新 / 重载间隔的唤醒信号
    pub refresh_signal: Notify,
    /// 刷新互斥：防止主循环与弹窗触发的刷新并发执行
    pub refresh_lock: Mutex<()>,
    /// 上次成功刷新时间（unix 秒）
    pub last_refresh: AtomicI64,
    /// 最新版本检查结果（有更新时 UI 显示更新按钮）
    pub update_info: Mutex<Option<UpdateInfo>>,
}

#[derive(Debug, Clone)]
pub struct NotifyRecord {
    pub key: String,
    pub last_at: i64,
    pub count: u32,
}

/// 弹窗 / 托盘悬停激活标记：任一为 true 时取消自动隐藏
pub static HOVER_ACTIVE: AtomicBool = AtomicBool::new(false);
/// 隐藏任务代号：每次调度递增，执行前比对，旧任务作废
pub static HIDE_GEN: AtomicU64 = AtomicU64::new(0);
/// 主窗口页面加载完成后是否需要显示（仅首次使用/无套餐时为 true）
pub static SHOW_MAIN_ON_LOAD: AtomicBool = AtomicBool::new(false);
