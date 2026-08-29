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
    /// 当前平台安装包直链（无匹配产物时为 None，前端回退打开 Releases 页）
    pub asset_url: Option<String>,
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
        asset_url: None,
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
                    let asset_url = json
                        .get("assets")
                        .and_then(|v| v.as_array())
                        .and_then(|assets| {
                            assets
                                .iter()
                                .filter_map(|a| {
                                    a.get("browser_download_url").and_then(|u| u.as_str())
                                })
                                .find(|u| asset_matches(u))
                                .map(String::from)
                        });
                    let has_update = !latest.is_empty() && is_newer(&latest, &current);
                    UpdateInfo { has_update, latest, url, asset_url, ..base }
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


/// 按当前平台挑选安装包资产名（GitHub Release asset）
fn asset_matches(url: &str) -> bool {
    let u = url.to_lowercase();
    #[cfg(target_os = "windows")]
    {
        u.ends_with("x64-setup.exe")
    }
    #[cfg(target_os = "macos")]
    {
        #[cfg(target_arch = "aarch64")]
        {
            u.ends_with("aarch64.dmg")
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            u.ends_with("x64.dmg")
        }
    }
    #[cfg(target_os = "linux")]
    {
        u.ends_with("amd64.appimage")
    }
}

/// 应用内直接下载当前平台安装包（进度经 update-download-progress 事件推送），
/// Windows 下载完成后自动启动安装器并退出应用；macOS/Linux 打开所在文件夹。
#[tauri::command]
pub async fn download_and_install(app: AppHandle, url: String) -> Result<(), String> {
    use std::io::Write;
    use tauri::Emitter;

    if !asset_matches(&url) {
        return Err("该链接不是当前平台的安装包".into());
    }
    let fname = url
        .split('/')
        .next_back()
        .unwrap_or("plan-limit-setup")
        .split('?')
        .next()
        .unwrap_or("plan-limit-setup")
        .to_string();
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("无法定位下载目录: {e}"))?;
    let dest = dir.join(&fname);

    let mut resp = http::client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_pct: i64 = -1;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("下载中断: {e}"))?
    {
        file.write_all(&chunk).map_err(|e| format!("写入失败: {e}"))?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = (downloaded * 100 / total) as i64;
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit("update-download-progress", pct);
            }
        }
    }
    file.flush().ok();
    drop(file);

    #[cfg(target_os = "windows")]
    {
        // 启动 NSIS 安装器（带向导，可改目录），随后退出本应用
        std::process::Command::new(&dest)
            .spawn()
            .map_err(|e| format!("启动安装器失败: {e}"))?;
        app.exit(0);
    }

    // macOS/Linux：无法静默安装，打开所在文件夹引导用户
    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_opener::OpenerExt;
        if let Some(folder) = dest.parent() {
            let _ = app.opener().open_path(folder.to_string_lossy(), None::<&str>);
        }
    }
    Ok(())
}
