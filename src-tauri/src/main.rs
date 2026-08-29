//! Coding Plan Limit — 聚合展示多个 Coding Plan / Token Plan 套餐剩余额度。
//!
//! 架构：
//!   usage/      各平台额度查询器（移植自 claude-mini-hud）
//!   store.rs    配置持久化 + 系统凭据库
//!   scheduler   定时刷新 + 阈值通知
//!   tray.rs     托盘 + 悬停弹窗
//!   commands.rs 前后端 IPC

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod scheduler;
mod state;
mod store;
mod tray;
mod update;
mod usage;

use tauri::{Manager, WindowEvent};
use state::AppState;

fn main() {
    tauri::Builder::default()
        // 第二实例启动时唤起已有实例的主窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_templates,
            commands::list_plans,
            commands::save_plan,
            commands::delete_plan,
            commands::get_views,
            commands::refresh_now,
            commands::get_settings,
            commands::save_settings,
            commands::open_main,
            commands::hide_popup,
            commands::set_popup_hover,
            commands::popup_size_changed,
            commands::quit_app,
            commands::set_custom_icon,
            commands::set_logo_style,
            commands::reset_custom_icon,
            commands::get_config_dir,
            commands::open_external,
            commands::reorder_plans,
            update::check_update,
            update::get_update_info,
        ])
        .setup(|app| {
            // 冷启动先载入上次快照，避免弹窗/主界面短暂空白
            {
                let snapshots = store::load_snapshots(app.handle());
                let state = app.state::<AppState>();
                let mut map = state.snapshots.blocking_lock();
                for (k, v) in snapshots {
                    map.insert(k, v);
                }
            }

            // 应用 Logo（自定义 > 单色 > 默认原色），同步托盘 + 主窗口标题栏图标
            {
                let settings = store::load_settings(app.handle());
                let img = if let Some(data_url) = settings.custom_icon {
                    data_url
                        .strip_prefix("data:image/png;base64,")
                        .and_then(|payload| {
                            base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                payload,
                            )
                            .ok()
                        })
                        .and_then(|raw| tauri::image::Image::from_bytes(&raw[..]).ok())
                } else if settings.logo_style == "mono" {
                    tauri::image::Image::from_bytes(commands::LOGO_MONO).ok()
                } else {
                    None
                };
                if let Some(img) = img {
                    commands::apply_tray_icon(app.handle(), img.clone());
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.set_icon(img);
                    }
                }
            }

            // Windows：弹窗 Mica/Acrylic 效果层设置 DWM 圆角
            tray::round_popup_corners(app.handle());

            // 同步开机自启状态
            {
                use tauri_plugin_autostart::ManagerExt;
                let settings = store::load_settings(app.handle());
                let autolaunch = app.autolaunch();
                if settings.autostart {
                    let _ = autolaunch.enable();
                } else {
                    let _ = autolaunch.disable();
                }
            }

            tray::create(app)?;
            scheduler::start(app.handle().clone());
            // 启动 8 秒后首查 + 每 24 小时后台无感检查更新
            update::start_auto_check(app.handle().clone());

            // 首次使用（还没有套餐）在页面加载完成后显示主窗口，否则静默驻留托盘
            state::SHOW_MAIN_ON_LOAD.store(
                store::load_plans(app.handle()).is_empty(),
                std::sync::atomic::Ordering::Relaxed,
            );
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // 关主窗口 = 最小化到托盘，不退出进程
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .on_page_load(|webview, payload| {
            // 页面渲染完成后按需显示（消费一次性标志，避免每次加载都弹出）
            if payload.event() == tauri::webview::PageLoadEvent::Finished
                && webview.label() == "main"
                && state::SHOW_MAIN_ON_LOAD.swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                if let Some(win) = webview.app_handle().get_webview_window("main") {
                    let _ = win.show();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Coding Plan Limit 启动失败");
}
