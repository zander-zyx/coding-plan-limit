//! 前端可调用的 IPC 命令。

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::scheduler;
use crate::state::AppState;
use crate::store;
use crate::tray;
use crate::usage::templates;
use crate::usage::types::{PlanConfig, PlanView, Settings, Template};

#[tauri::command]
pub fn list_templates() -> Vec<Template> {
    templates()
}

#[tauri::command]
pub fn list_plans(app: AppHandle) -> Vec<PlanConfig> {
    store::load_plans(&app)
}

/// 保存套餐时随附的密钥明文（为空 / None 表示不修改）
#[derive(Deserialize, Default)]
pub struct SecretInput {
    pub bearer: Option<String>,
    pub cookie: Option<String>,
    pub ak_id: Option<String>,
    pub ak_secret: Option<String>,
}

#[derive(serde::Serialize)]
pub struct SavePlanOut {
    pub plan: PlanConfig,
    /// 凭据库不可用降级为明文存储时的警告
    pub warning: Option<String>,
}

#[tauri::command]
pub fn save_plan(
    app: AppHandle,
    plan: PlanConfig,
    secret: Option<SecretInput>,
) -> Result<SavePlanOut, String> {
    let mut plan = plan;

    let is_new = plan.id.trim().is_empty();
    if is_new {
        plan.id = uuid::Uuid::new_v4().to_string();
        plan.created_at = crate::usage::types::now_secs();
    }
    if plan.name.trim().is_empty() {
        return Err("套餐名称不能为空".into());
    }

    plan.name = plan.name.trim().chars().take(50).collect();

    // 阈值夹紧：窗口/固定额度型 0-100，余额型 >=0；非法输入回默认 10
    if !plan.threshold.is_finite() {
        plan.threshold = 10.0;
    }
    let quota_type = crate::usage::templates()
        .into_iter()
        .find(|t| t.id == plan.template)
        .map(|t| t.quota_type)
        .unwrap_or_default();
    plan.threshold = if quota_type == "balance" {
        plan.threshold.max(0.0)
    } else {
        plan.threshold.clamp(0.0, 100.0)
    };

    // 模板变更时清掉旧模板的密钥（同认证类型不删，如 Kimi 余额 ↔ Coding Plan 切换）
    let old_plan = if is_new {
        None
    } else {
        store::load_plans(&app).into_iter().find(|p| p.id == plan.id)
    };
    if let Some(old) = &old_plan {
        if old.template != plan.template {
            let auth_of = |id: &str| {
                crate::usage::templates()
                    .into_iter()
                    .find(|t| t.id == id)
                    .map(|t| t.auth)
            };
            if auth_of(&old.template) != auth_of(&plan.template) {
                store::delete_secret(&app, &plan.id);
            }
        }
    }

    // 先写密钥（凭据库不可用时会写入 config 的兜底区）
    let mut warnings: Vec<String> = Vec::new();
    if let Some(sec) = secret {
        let fields = [
            ("key", sec.bearer),
            ("cookie", sec.cookie),
            ("ak_id", sec.ak_id),
            ("ak_secret", sec.ak_secret),
        ];
        for (suffix, value) in fields {
            if let Some(v) = value.filter(|s| !s.trim().is_empty()) {
                if let Some(w) = store::save_secret(&app, &plan.id, suffix, &v)? {
                    warnings.push(w);
                }
            }
        }
    }

    // 密钥写入之后再加锁更新套餐列表（避免旧快照覆盖刚写入的兜底密钥）。
    // 已存在的套餐原位替换（保留拖拽顺序），仅新增时追加。
    store::update_config(&app, |config| {
        if let Some(slot) = config.plans.iter_mut().find(|p| p.id == plan.id) {
            *slot = plan.clone();
        } else {
            config.plans.push(plan.clone());
        }
    })?;

    // 仅启用状态变化（开关切换）→ 静默保存：不唤醒调度全量刷新、不广播。
    // 前端开关已本地切换；弹窗打开时会自行 get_views 拉最新状态。
    let only_toggle = old_plan.map_or(false, |old| {
        let mut a = serde_json::to_value(&old).unwrap_or(serde_json::Value::Null);
        let mut b = serde_json::to_value(&plan).unwrap_or(serde_json::Value::Null);
        let enabled = b.get("enabled").cloned().unwrap_or(serde_json::Value::Null);
        a["enabled"] = enabled.clone();
        b["enabled"] = enabled;
        a == b
    });
    if !only_toggle {
        scheduler::request_refresh(&app);
    }
    Ok(SavePlanOut {
        plan,
        warning: warnings.into_iter().next_back(),
    })
}

#[tauri::command]
pub fn delete_plan(app: AppHandle, id: String) -> Result<(), String> {
    store::update_config(&app, |config| {
        config.plans.retain(|p| p.id != id);
    })?;
    store::delete_secret(&app, &id);

    // 清理内存快照/通知记录 + 磁盘快照缓存
    {
        if let Ok(mut map) = app.state::<AppState>().snapshots.try_lock() {
            map.remove(&id);
        }
        if let Ok(mut notified) = app.state::<AppState>().notified.try_lock() {
            notified.remove(&id);
        }
    }
    let mut disk = store::load_snapshots(&app);
    if disk.remove(&id).is_some() {
        store::save_snapshots(&app, &disk);
    }

    emit_views(&app);
    Ok(())
}

#[tauri::command]
pub async fn get_views(app: AppHandle) -> Vec<PlanView> {
    scheduler::build_views(&app).await
}

#[tauri::command]
pub fn refresh_now(app: AppHandle) {
    scheduler::request_refresh(&app);
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    store::load_settings(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let old_refresh = store::load_settings(&app).refresh_seconds;
    store::update_config(&app, |config| {
        config.settings = settings;
    })?;

    // 同步开机自启
    {
        use tauri_plugin_autostart::ManagerExt;
        let autolaunch = app.autolaunch();
        let enabled = store::load_settings(&app).autostart;
        if enabled {
            autolaunch.enable().map_err(|e| format!("设置开机自启失败: {e}"))?;
        } else {
            let _ = autolaunch.disable();
        }
    }

    // 广播设置变更（弹窗实时跟随主题色/主题/进度样式）
    let latest = store::load_settings(&app);
    let _ = tauri::Emitter::emit(&app, "settings-updated", &latest);

    // 仅刷新间隔变化时才需要唤醒调度循环（避免改主题色也触发全量 API 刷新）
    if store::load_settings(&app).refresh_seconds != old_refresh {
        scheduler::request_refresh(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn open_main(app: AppHandle) {
    tray::show_main(&app);
}

#[tauri::command]
pub fn hide_popup(app: AppHandle) {
    if let Some(win) = app.get_webview_window("popup") {
        let _ = win.hide();
    }
}

/// 弹窗 JS 上报鼠标进入/离开（悬停状态判定，防止移入弹窗时被自动隐藏）
#[tauri::command]
pub fn set_popup_hover(app: AppHandle, active: bool) {
    if !active {
        tray::schedule_hide(&app);
    }
}

/// 用系统浏览器打开链接（弹窗点击套餐卡片跳转官网 / About 页仓库链接）
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("仅允许打开 http(s) 链接".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开链接失败: {e}"))
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// 拖拽排序：按前端传来的 id 顺序重排（未列出的套餐保持相对顺序排在后面）
#[tauri::command]
pub fn reorder_plans(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    store::update_config(&app, |config| {
        let mut rank = std::collections::HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            rank.insert(id.clone(), i);
        }
        config.plans.sort_by(|a, b| {
            let ra = rank.get(&a.id).copied();
            let rb = rank.get(&b.id).copied();
            match (ra, rb) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                // 都未列出：按原有 created_at 相对顺序
                (None, None) => a.created_at.cmp(&b.created_at),
            }
        });
    })?;
    emit_views(&app);
    Ok(())
}

#[tauri::command]
pub fn get_config_dir(app: AppHandle) -> Result<String, String> {
    store::config_dir(&app).map(|p| p.to_string_lossy().into_owned())
}

// ─── 自定义托盘图标 ────────────────────────────────────────────────────────

/// 前端把用户选择的图片画成 PNG dataURL 后提交；Rust 解码并实时更换托盘图标
#[tauri::command]
pub fn set_custom_icon(app: AppHandle, data_url: String) -> Result<(), String> {
    let payload = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or("图标仅支持 PNG 格式")?;
    let raw = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        payload,
    )
    .map_err(|e| format!("图标数据解码失败: {e}"))?;
    let img = tauri::image::Image::from_bytes(&raw[..])
        .map_err(|e| format!("图标解析失败: {e}"))?;
    apply_tray_icon(&app, img.clone());
    // 同步主窗口标题栏/任务栏图标（任务栏常驻图标仍以 exe 内置图标为准，运行时无法替换）
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_icon(img);
    }

    store::update_config(&app, |config| {
        config.settings.custom_icon = Some(data_url);
    })?;
    Ok(())
}

#[tauri::command]
pub fn reset_custom_icon(app: AppHandle) -> Result<(), String> {
    if let Some(default_icon) = app.default_window_icon() {
        // 托盘与主窗口（任务栏）同步恢复默认
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_icon(Some(default_icon.clone()));
        }
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_icon(default_icon.clone());
        }
    }
    store::update_config(&app, |config| {
        config.settings.custom_icon = None;
    })?;
    Ok(())
}

pub fn apply_tray_icon(app: &AppHandle, img: tauri::image::Image<'_>) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_icon(Some(img));
    }
}

/// 删除套餐后单独推送视图（无需等刷新）
fn emit_views(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        scheduler::emit_views(&app).await;
    });
}
