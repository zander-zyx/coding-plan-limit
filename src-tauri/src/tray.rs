//! 托盘图标 + 悬停弹窗控制。
//! Windows：悬停即显示；macOS / Linux：左键点击切换（系统托盘 API 不支持悬停事件）。

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Rect};

use crate::scheduler;
use crate::state::{HOVER_ACTIVE, HIDE_GEN};

/// 屏幕底部安全边距（Windows 任务栏高度 ~48px，留余量）
const TASKBAR_SAFE: f64 = 64.0;

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开主界面", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().expect("缺少应用图标").clone())
        .tooltip("Coding Plan Limit — 悬停查看额度")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "refresh" => scheduler::request_refresh(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // Windows 悬停显示
                TrayIconEvent::Enter { rect, .. } => {
                    scheduler::set_hover(true);
                    show_popup_at(app, rect.clone());
                }
                TrayIconEvent::Leave { .. } => {
                    scheduler::set_hover(false);
                    schedule_hide(app);
                }
                // macOS / Linux 点击切换
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } => {
                    if let Some(popup) = app.get_webview_window("popup") {
                        if popup.is_visible().unwrap_or(false) {
                            let _ = popup.hide();
                        } else {
                            scheduler::set_hover(true);
                            show_popup_at(app, rect.clone());
                        }
                    }
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// 从 Rect（Position/Size 均为枚举）提取物理坐标 (x, y, w, h)
fn rect_xywh(rect: &Rect) -> (f64, f64, f64, f64) {
    let (x, y) = match &rect.position {
        tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
        tauri::Position::Logical(p) => (p.x, p.y),
    };
    let (w, h) = match &rect.size {
        tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
        tauri::Size::Logical(s) => (s.width, s.height),
    };
    (x, y, w, h)
}

/// 在托盘图标旁定位并显示弹窗
pub fn show_popup_at(app: &AppHandle, rect: Rect) {
    // 递增代号，取消尚未执行的隐藏任务
    HIDE_GEN.fetch_add(1, Ordering::Relaxed);

    let Some(win) = app.get_webview_window("popup") else {
        return;
    };
    let size = win.outer_size().unwrap_or_default();
    let (w, h) = (size.width as f64, size.height as f64);

    let (rx, ry, rw, rh) = rect_xywh(&rect);
    let mut x = rx + rw / 2.0 - w / 2.0;
    let mut y = ry - h - 12.0;
    if y < 8.0 {
        // 托盘在顶部（macOS 菜单栏）→ 显示在图标下方
        y = ry + rh + 12.0;
    }
    // 收拢进屏幕工作区内（上界与下界同基准，避免 clamp(min>max) panic；
    // 底部预留任务栏高度，防止弹窗压住任务栏/托盘 tooltip）
    if let Ok(Some(monitor)) = app.monitor_from_point(rx, ry) {
        let mpos = monitor.position();
        let msize = monitor.size();
        let lo_x = mpos.x as f64 + 8.0;
        let hi_x = ((mpos.x + msize.width as i32) as f64 - w - 8.0).max(lo_x);
        let lo_y = mpos.y as f64 + 8.0;
        let hi_y = ((mpos.y + msize.height as i32) as f64 - h - 8.0 - TASKBAR_SAFE)
            .max(lo_y);
        x = x.clamp(lo_x, hi_x);
        y = y.clamp(lo_y, hi_y);
    }

    let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        x as i32, y as i32,
    )));
    let _ = win.show();

    // 每次打开弹窗都强制刷新一轮（refresh_lock 互斥，悬停连击不会并发轰炸）
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        scheduler::refresh_all(&app).await;
        scheduler::emit_views(&app).await;
    });
}

/// 延迟隐藏弹窗：期间鼠标进入弹窗（HOVER_ACTIVE）或再次悬停托盘则取消
pub fn schedule_hide(app: &AppHandle) {
    let gen = HIDE_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(650)).await;
        if HIDE_GEN.load(Ordering::Relaxed) == gen && !HOVER_ACTIVE.load(Ordering::Relaxed) {
            if let Some(win) = app.get_webview_window("popup") {
                let _ = win.hide();
            }
        }
    });
}
