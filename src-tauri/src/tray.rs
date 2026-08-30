//! 托盘图标 + 悬停弹窗控制。
//! Windows：悬停即显示；macOS / Linux：左键点击切换（系统托盘 API 不支持悬停事件）。

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Rect};

use crate::scheduler;
use crate::state::HIDE_GEN;

/// 最近一次托盘图标区域 (x, y, w, h)（物理像素），供弹窗高度变化后重新定位
pub static LAST_TRAY_RECT: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);
/// 最近一次弹窗触发重推/刷新的时刻（unix 秒）：冷却内重开沿用现有画面
pub static LAST_POPUP_SHOW: AtomicI64 = AtomicI64::new(0);

/// 屏幕底部安全边距（Windows 任务栏高度 ~48px，留余量）
const TASKBAR_SAFE: f64 = 64.0;
/// 弹窗打开时强制刷新的节流间隔（秒）：悬停连击不会轰炸 API
const POPUP_REFRESH_THROTTLE: i64 = 60;
/// 移出弹窗后的判定延时（毫秒）：光标仍在弹窗附近则不隐藏
const HIDE_DELAY_MS: u64 = 600;
/// 弹窗附近的判定边距（物理像素）：从托盘移向弹窗的途中不算"移出"
const HIDE_MARGIN: f64 = 48.0;

/// Windows：给弹窗窗口设置 DWM 圆角（Mica/Acrylic 效果层不会自动圆角），
/// 并隐藏 DWM 附带的 1px 系统边框——DWM 圆角(8px)小于 CSS 面板圆角(24px)，
/// 系统边框会在四个角以"幽灵框"形态露出。
#[cfg(windows)]
pub fn round_popup_corners(app: &AppHandle) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    if let Some(win) = app.get_webview_window("popup") {
        if let Ok(hwnd) = win.hwnd() {
            unsafe {
                let pref = DWMWCP_ROUND as u32;
                let _ = DwmSetWindowAttribute(
                    hwnd.0,
                    DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    &pref as *const u32 as *const core::ffi::c_void,
                    4,
                );
                let none = DWMWA_COLOR_NONE;
                let _ = DwmSetWindowAttribute(
                    hwnd.0,
                    DWMWA_BORDER_COLOR as u32,
                    &none as *const u32 as *const core::ffi::c_void,
                    4,
                );
            }
        }
    }
}

#[cfg(not(windows))]
pub fn round_popup_corners(_app: &AppHandle) {}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "主界面", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "更新数据", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &quit])?;

    // 初始图标按设置解析（custom 头像 / Mark），无则用应用默认原色图标——
    // 启动恢复必须在这里做：setup 阶段托盘尚未创建，set_icon 会静默失败
    let icon = crate::commands::resolve_saved_logo(app.handle())
        .unwrap_or_else(|| app.default_window_icon().expect("缺少应用图标").clone());

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("查看额度")
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
                    show_popup_at(app, rect.clone());
                }
                TrayIconEvent::Leave { .. } => {
                    schedule_hide(app);
                }
                // macOS / Linux 点击切换（Windows 由 Enter 悬停驱动，
                // 点击会派发 Click 造成 显示→隐藏 闪烁，忽略）
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } => {
                    if cfg!(windows) {
                        return;
                    }
                    if let Some(popup) = app.get_webview_window("popup") {
                        if popup.is_visible().unwrap_or(false) {
                            let _ = popup.hide();
                        } else {
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
        // show 前补设：任务栏按钮随 show 创建，创建时图标即为最新值
        crate::commands::prime_window_icon(app);
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

    // 记住托盘区域：弹窗内容高度变化后（popup_size_changed）按它重新定位
    *LAST_TRAY_RECT.lock().unwrap() = Some(rect_xywh(&rect));

    position_popup(app);
    let _ = win.show();

    // 冷却内重开（悬停连击/移出再进入）：窗口隐藏非销毁，画面原样保留，
    // 不重推视图也不触发刷新，避免每次进入都重渲染重播进度条动画；0 = 关闭
    let cooldown = crate::store::load_settings(app).popup_cooldown_secs as i64;
    if cooldown > 0 {
        let now = crate::usage::types::now_secs();
        if now - LAST_POPUP_SHOW.load(Ordering::Relaxed) < cooldown {
            return;
        }
        LAST_POPUP_SHOW.store(now, Ordering::Relaxed);
    }

    // 打开弹窗：数据 60s 内刷新过就只重推视图，否则强制刷新一轮（互斥防并发）
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let stale = {
            let state = app.state::<crate::state::AppState>();
            crate::usage::types::now_secs() - state.last_refresh.load(Ordering::Relaxed)
                > POPUP_REFRESH_THROTTLE
        };
        if stale {
            scheduler::refresh_all(&app).await;
        } else {
            scheduler::emit_views(&app).await;
        }
    });
}

/// 按最近托盘区域重新定位弹窗（底部锚定托盘上方，收拢进屏幕工作区）
pub fn position_popup(app: &AppHandle) {
    let Some((rx, ry, rw, rh)) = *LAST_TRAY_RECT.lock().unwrap() else {
        return;
    };
    let Some(win) = app.get_webview_window("popup") else {
        return;
    };
    let size = win.outer_size().unwrap_or_default();
    let (w, h) = (size.width as f64, size.height as f64);

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
}

/// 延迟隐藏弹窗：到时用"光标是否在弹窗附近"判定（JS mouseleave 在快速移出时
/// 可能不派发，光标判定是唯一可靠事实源）。
/// 光标仍在弹窗附近 → 原地重排一轮复检（不依赖 JS 事件兜底）；
/// 光标已离开 → 隐藏。
pub fn schedule_hide(app: &AppHandle) {
    let gen = HIDE_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(HIDE_DELAY_MS)).await;
        if HIDE_GEN.load(Ordering::Relaxed) != gen {
            return; // 期间又悬停/进入，本轮作废
        }
        let Some(win) = app.get_webview_window("popup") else { return };
        if !win.is_visible().unwrap_or(false) {
            return;
        }
        if let (Ok(cursor), Ok(pos), Ok(size)) =
            (app.cursor_position(), win.outer_position(), win.outer_size())
        {
            let (cx, cy) = (cursor.x, cursor.y);
            let (x, y) = (pos.x as f64, pos.y as f64);
            let (w, h) = (size.width as f64, size.height as f64);
            let near = cx >= x - HIDE_MARGIN
                && cx <= x + w + HIDE_MARGIN
                && cy >= y - HIDE_MARGIN
                && cy <= y + h + HIDE_MARGIN;
            if near {
                // 仍在弹窗附近（含从托盘移向弹窗途中）：重排一轮复检
                schedule_hide(&app);
                return;
            }
        }
        let _ = win.hide();
    });
}
