//! 后台调度：启动即刷新 → 等待间隔/唤醒信号 → 循环；
//! 刷新后按阈值发系统通知。

use std::sync::atomic::Ordering;
use std::time::Duration;

use futures::future::join_all;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, NotifyRecord, HOVER_ACTIVE};
use crate::store;
use crate::usage::types::{NotifyMode, PlanConfig, PlanView, Quota, Snapshot};
use crate::usage;

/// 距上次刷新超过该秒数，弹窗打开时才触发即时刷新（节流）
const STALE_SECS: i64 = 120;

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_all(&app).await;
            wait_interval(&app).await;
        }
    });
}

/// 等待 min(设置间隔, 下一次唤醒信号)。设置变更通过唤醒信号即时生效，
/// 因此每个周期开始时读取一次设置即可，无需每秒轮询磁盘。
async fn wait_interval(app: &AppHandle) {
    let secs = (store::load_settings(app).refresh_seconds.max(10)) as u64;
    let mut elapsed: u64 = 0;
    loop {
        let state = app.state::<AppState>();
        tokio::select! {
            _ = state.refresh_signal.notified() => return,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        elapsed += 1;
        if elapsed >= secs {
            return;
        }
    }
}

/// 触发一次刷新（非阻塞）。notify_one 会驻留许可：
/// 即使刷新正在进行，等待方也能在结束后立即收到信号。
pub fn request_refresh(app: &AppHandle) {
    app.state::<AppState>().refresh_signal.notify_one();
}

/// 弹窗打开时调用：数据陈旧才刷新
pub async fn refresh_if_stale(app: &AppHandle) {
    let last = app.state::<AppState>().last_refresh.load(Ordering::Relaxed);
    if crate::usage::types::now_secs() - last > STALE_SECS {
        refresh_all(app).await;
    }
}

/// 并发刷新所有启用的套餐 → 更新状态/缓存 → 推送视图 → 阈值通知。
/// refresh_lock 互斥：主循环与弹窗触发撞车时后来者直接跳过（主循环兜底）。
pub async fn refresh_all(app: &AppHandle) {
    // State 绑定保活到函数结束，MutexGuard 才能跨 await 持有
    let state = app.state::<AppState>();
    let Ok(_guard) = state.refresh_lock.try_lock() else {
        return;
    };

    let plans = store::load_plans(app);
    let enabled: Vec<PlanConfig> = plans.iter().filter(|p| p.enabled).cloned().collect();

    let futs = enabled.iter().map(|plan| {
        let cred = store::load_credential(app, plan);
        async move { usage::snapshot(plan, &cred).await }
    });
    let snapshots: Vec<Snapshot> = join_all(futs).await;

    {
        let state = app.state::<AppState>();
        let mut map = state.snapshots.lock().await;
        for snap in &snapshots {
            map.insert(snap.plan_id.clone(), snap.clone());
        }
        state
            .last_refresh
            .store(crate::usage::types::now_secs(), Ordering::Relaxed);
    }

    let merged = {
        let mut map = store::load_snapshots(app);
        for s in &snapshots {
            map.insert(s.plan_id.clone(), s.clone());
        }
        map
    };
    store::save_snapshots(app, &merged);

    emit_views(app).await;
    check_thresholds(app, &enabled, &snapshots).await;
}

/// 构建视图并推送到所有窗口
pub async fn emit_views(app: &AppHandle) {
    let views = build_views(app).await;
    let _ = app.emit("views-updated", &views);
}

pub async fn build_views(app: &AppHandle) -> Vec<PlanView> {
    let plans = store::load_plans(app);
    let state = app.state::<AppState>();
    let map = state.snapshots.lock().await;
    plans
        .iter()
        .map(|p| PlanView {
            plan: p.clone(),
            snapshot: map.get(&p.id).cloned(),
        })
        .collect()
}

/// 阈值检查 + 系统通知。
/// 语义：窗口/固定额度型 —— 剩余百分比 ≤ threshold 触发；余额型 —— 余额 ≤ threshold 触发。
/// 通知频率（全局设置）三选一：
///   Off      —— 不通知
///   Interval —— 同一告警每 N 分钟重复提醒
///   Count    —— 同一告警每 N 次刷新提醒一次
/// 告警键变化（新窗口周期 / 重置）则立即重新提醒。
async fn check_thresholds(app: &AppHandle, plans: &[PlanConfig], snapshots: &[Snapshot]) {
    use tauri_plugin_notification::NotificationExt;

    let settings = store::load_settings(app);
    if settings.notify_mode == NotifyMode::Off {
        return;
    }
    let now = crate::usage::types::now_secs();

    let state = app.state::<AppState>();
    let mut notified = state.notified.lock().await;

    for plan in plans {
        let Some(snap) = snapshots.iter().find(|s| s.plan_id == plan.id) else {
            continue;
        };
        let Some(quota) = &snap.quota else { continue };

        let hit = match quota {
            Quota::Windows { windows } => {
                // 取剩余最少的窗口（任一窗口越限即告警）
                windows
                    .iter()
                    .min_by(|a, b| {
                        (100.0 - a.used_percent).total_cmp(&(100.0 - b.used_percent))
                    })
                    .filter(|w| (100.0 - w.used_percent) <= plan.threshold)
                    .map(|w| {
                        let reset = w
                            .reset_at
                            .and_then(human_delta)
                            .map(|d| format!("，{d}后重置"))
                            .unwrap_or_default();
                        (
                            format!("{}/{}", w.label, w.reset_at.unwrap_or(0)),
                            format!(
                                "「{}」剩余仅 {:.0}%{}",
                                w.label,
                                (100.0 - w.used_percent).max(0.0),
                                reset
                            ),
                        )
                    })
            }
            Quota::FixedQuota {
                used_percent,
                reset_at,
                ..
            } => {
                let remaining = 100.0 - used_percent;
                if remaining > plan.threshold {
                    None
                } else {
                    Some((
                        format!("fixed/{}", reset_at.unwrap_or(0)),
                        format!("额度剩余仅 {remaining:.0}%"),
                    ))
                }
            }
            Quota::Balance {
                amount, currency, ..
            } => {
                if *amount > plan.threshold {
                    None
                } else {
                    Some((
                        "balance".into(),
                        format!("余额仅剩 {currency} {amount:.2}"),
                    ))
                }
            }
        };

        let Some((key, msg)) = hit else {
            // 告警解除：清除记录，下次再触发可立即提醒
            notified.remove(&plan.id);
            continue;
        };

        // 判定本次是否该提醒（不提前刷新时间戳）
        let do_notify = match settings.notify_mode {
            NotifyMode::Interval => {
                let interval = (settings.notify_interval_minutes.max(1) as i64) * 60;
                match notified.get(&plan.id) {
                    Some(r) if r.key == key => now - r.last_at >= interval,
                    _ => true, // 新告警键 → 立即提醒
                }
            }
            NotifyMode::Count => {
                let mut hit = false;
                match notified.get_mut(&plan.id) {
                    Some(r) if r.key == key => {
                        r.count += 1;
                        if r.count >= settings.notify_count.max(1) {
                            r.count = 0;
                            hit = true;
                        }
                    }
                    Some(r) => {
                        // 告警键变化：重置计数，立即提醒
                        r.key = key.clone();
                        r.count = 0;
                        hit = true;
                    }
                    None => hit = true,
                }
                hit
            }
            NotifyMode::Off => false, // 已在函数入口提前返回
        };

        // 维护记录：last_at 仅在真正发出通知时刷新，否则 Interval 模式永不重复提醒
        {
            let rec = notified
                .entry(plan.id.clone())
                .or_insert_with(|| NotifyRecord {
                    key: key.clone(),
                    last_at: now,
                    count: 0,
                });
            rec.key = key.clone();
            if do_notify {
                rec.last_at = now;
            }
        }

        if !do_notify {
            continue;
        }

        let _ = app
            .notification()
            .builder()
            .title(format!("{} 额度告警", plan.name))
            .body(msg)
            .show();
    }
}

fn human_delta(unix_secs: i64) -> Option<String> {
    let diff = unix_secs - crate::usage::types::now_secs();
    if diff <= 0 {
        return None;
    }
    let h = diff / 3600;
    let m = (diff % 3600) / 60;
    if h > 0 {
        Some(format!("{h}小时{m}分"))
    } else {
        Some(format!("{m}分钟"))
    }
}

/// 供托盘模块读写悬停标记
pub fn set_hover(active: bool) {
    HOVER_ACTIVE.store(active, Ordering::Relaxed);
}
