//! 检查更新：查询 GitHub Releases 最新版本并与当前版本比对。

use serde::Serialize;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::usage::http;

const RELEASES_API: &str =
    "https://api.github.com/repos/zander-zyx/coding-plan-limit/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/zander-zyx/coding-plan-limit/releases";
const AUTO_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
    /// 比对失败时的原因（网络不通等），前端给出提示
    pub error: Option<String>,
}

fn parse_version(tag: &str) -> Vec<u64> {
    tag.trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.trim().parse().unwrap_or(0))
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let l = parse_version(latest);
    let c = parse_version(current);
    for i in 0..3 {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv != cv {
            return lv > cv;
        }
    }
    false
}

/// 查询最新版本信息（供命令与托盘菜单共用）
pub async fn latest_update() -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let base = UpdateInfo {
        has_update: false,
        current: current.clone(),
        latest: String::new(),
        url: RELEASES_PAGE.to_string(),
        error: None,
    };

    match http::client()
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("coding-plan-limit/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if !resp.status().is_success() {
                return UpdateInfo {
                    error: Some(format!("GitHub API HTTP {}", resp.status().as_u16())),
                    ..base
                };
            }
            let text = resp
                .text()
                .await
                .map_err(|e| format!("读取响应失败: {e}"))
                .and_then(|t| {
                    serde_json::from_str::<serde_json::Value>(&t)
                        .map_err(|e| format!("响应解析失败: {e}"))
                });
            match text {
                Ok(json) => {
                    let latest = json
                        .get("tag_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let url = json
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or(RELEASES_PAGE)
                        .to_string();
                    let has_update = !latest.is_empty() && is_newer(&latest, &current);
                    UpdateInfo { has_update, latest, url, ..base }
                }
                Err(e) => UpdateInfo { error: Some(e), ..base },
            }
        }
        Err(e) => UpdateInfo { error: Some(format!("网络请求失败: {e}")), ..base },
    }
}

/// 将结果写入共享状态并广播（主界面/弹窗据此显示更新按钮）
async fn publish_update(app: &AppHandle, info: &UpdateInfo) {
    if !info.has_update {
        return;
    }
    {
        let state = app.state::<AppState>();
        *state.update_info.lock().await = Some(info.clone());
    }
    let _ = app.emit("update-available", info);
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> UpdateInfo {
    let info = latest_update().await;
    publish_update(&app, &info).await;
    info
}

/// 前端启动时查询当前已知的更新状态（不发起网络请求）
#[tauri::command]
pub async fn get_update_info(app: AppHandle) -> Option<UpdateInfo> {
    app.state::<AppState>().update_info.lock().await.clone()
}

/// 启动 8 秒后检查一次，之后每 24 小时后台无感检查；
/// 有新版本且该版本未提醒过时才发系统通知（不自动打开浏览器）。
pub fn start_auto_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        loop {
            silent_check(&app).await;
            tokio::time::sleep(AUTO_CHECK_INTERVAL).await;
        }
    });
}

async fn silent_check(app: &AppHandle) {
    if !crate::store::load_settings(app).auto_check_update {
        return;
    }
    let info = latest_update().await;
    if !info.has_update {
        return;
    }
    publish_update(app, &info).await;

    // 同一版本只发一次系统通知
    let notified = crate::store::load_config(app).notified_update;
    if notified.as_deref() == Some(info.latest.as_str()) {
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(format!("发现新版本 v{}", info.latest))
        .body(format!("当前 v{}，主界面右上角可前往更新", info.current))
        .show();
    let _ = crate::store::update_config(app, |c| {
        c.notified_update = Some(info.latest.clone());
    });
}
