//! Coding Plan Limit — 聚合展示多个 Coding Plan / Token Plan 套餐剩余额度。
//!
//! 架构：
//!   usage/      各平台额度查询器（移植自 claude-mini-hud）
//!   store.rs    配置持久化 + 系统凭据库
//!   scheduler   定时刷新 + 阈值通知
//!   tray.rs     托盘 + 悬停弹窗
//!   commands.rs 前后端 IPC

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod codex_oauth;
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
    // 最早时机设进程级 AUMID：与开始菜单快捷方式解耦，任务栏按钮改用窗口图标
    commands::init_process_aumid();

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
            commands::codex_login_start,
            commands::codex_login_poll,
            commands::codex_accounts,
            commands::codex_account_delete,
            commands::codex_capture_for_plan,
            commands::codex_bind_plan,
            update::check_update,
            update::get_update_info,
            update::download_and_install,
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

            // 应用主窗口 Logo（按 logo_style：custom > mark > 默认原色），窗口级 + 类级
            // 图标双设——任务栏按钮在窗口 show 时读取类图标；
            // 托盘初始图标在 tray::create 内按同一设置解析（托盘创建前调
            // apply_tray_icon 会因 tray 尚不存在而静默丢失，曾有此 bug）
            {
                commands::debug_log(
                    app.handle(),
                    &format!("startup: process_aumid hr={}", commands::process_aumid_hr()),
                );
                let style = store::load_settings(app.handle()).logo_style;
                if let Some(img) = commands::resolve_saved_logo(app.handle()) {
                    commands::apply_window_icon(app.handle(), &img);
                }
                // macOS：Dock 图标跟随三态（其他平台空操作）
                commands::apply_dock_icon_for(app.handle(), &style);
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
                    // show 前补设：任务栏按钮随 show 创建，创建时图标即为最新值
                    commands::prime_window_icon(webview.app_handle());
                    let _ = win.show();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Coding Plan Limit 启动失败");
}
