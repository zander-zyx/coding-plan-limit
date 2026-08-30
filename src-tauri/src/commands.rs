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

/// 弹窗内容高度变化后由前端调用：按最近托盘区域重新定位（保持底部锚定托盘）
#[tauri::command]
pub fn popup_size_changed(app: AppHandle) {
    tray::position_popup(&app);
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

// ─── Logo（托盘 / 主窗口 / 侧边栏同步） ────────────────────────────────────

/// 内置 Logo Mark（用户设计的 Z 标，256px 深色圆角底）
pub static LOGO_MARK: &[u8] = include_bytes!("../../ui/icons/app-mark.png");

/// 默认原色图标 PNG 字节（macOS Dock 图标用：NSImage 吃编码图字节，非 RGBA 裸像素）
#[cfg(target_os = "macos")]
pub static DEFAULT_ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");

/// PNG dataURL → 原始 PNG 字节（macOS Dock 图标直接复用编码字节，免去重编码）
fn decode_data_url_png_bytes(data_url: &str) -> Option<Vec<u8>> {
    data_url
        .strip_prefix("data:image/png;base64,")
        .and_then(|payload| {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload).ok()
        })
}

/// PNG dataURL → 解码为图像
fn decode_data_url_png(data_url: &str) -> Option<tauri::image::Image<'static>> {
    decode_data_url_png_bytes(data_url)
        .and_then(|raw| tauri::image::Image::from_bytes(&raw[..]).ok())
}

/// 按当前设置解析应使用的 Logo 图（custom 图 / Mark；None = 默认原色）
pub fn resolve_saved_logo(app: &AppHandle) -> Option<tauri::image::Image<'static>> {
    let settings = store::load_settings(app);
    let style = settings.logo_style.clone();
    let custom = settings.custom_icon.clone();
    let img = match style.as_str() {
        "custom" => custom.as_deref().and_then(decode_data_url_png),
        "mark" => tauri::image::Image::from_bytes(LOGO_MARK).ok(),
        _ => None,
    };
    if img.is_none() && style != "color" {
        debug_log(app, &format!("resolve_logo: style={style} 解析失败，回落默认"));
    }
    img
}

/// 图标链路诊断日志（配置目录 icon-debug.log）：托盘/任务栏图标问题排查用
pub fn debug_log(app: &AppHandle, msg: &str) {
    use std::io::Write as _;
    let Ok(dir) = store::config_dir(app) else { return };
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("icon-debug.log"))
    else {
        return;
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(f, "[{ts}] {msg}");
}

/// Windows 原生窗口图标：Win11 任务栏按钮在创建时读取窗口类图标，且不随
/// WM_SETICON 刷新（表现为"标题栏已变、按钮仍旧图"）。因此对类图标
/// （SetClassLongPtrW）与窗口图标（WM_SETICON）双通道同设。
#[cfg(windows)]
mod win_icon {
    use windows_sys::Win32::Foundation::{HWND, PROPERTYKEY};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
    };
    use windows_sys::Win32::System::Com::StructuredStorage::{
        PROPVARIANT, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows_sys::Win32::UI::Shell::PropertiesSystem::SHGetPropertyStoreForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, GetSystemMetrics, SendMessageW, SetClassLongPtrW, SetPropW, GCLP_HICON,
        GCLP_HICONSM, HICON, ICONINFO, ICON_BIG, ICON_SMALL, SM_CXICON, SM_CXSMICON, SM_CYICON,
        SM_CYSMICON, WM_SETICON,
    };

    /// 窗口级 AUMID：与开始菜单快捷方式（其图标 = exe 内嵌图标）脱钩，
    /// 任务栏按钮回落为窗口图标（类图标已设为当前 Logo）。实测移除快捷方式后
    /// 按钮立即跟随窗口图标，此属性即用于不删快捷方式达到同样效果。
    const WINDOW_AUMID: &str = "com.zander.coding-plan-limit.main";
    /// PKEY_AppUserModel_ID（propkey.h，windows-sys 未导出常量）
    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows_sys::core::GUID {
            data1: 0x9F4C_2855,
            data2: 0x9F3E,
            data3: 0x4144,
            data4: [0x9C, 0x3A, 0x9C, 0x6C, 0x41, 0xC7, 0xC6, 0xC5],
        },
        pid: 5,
    };
    /// IID_IPropertyStore {886D8EEB-8CF2-4446-8D02-CDBA1DBDCF4B}
    const IID_IPROPERTYSTORE: windows_sys::core::GUID = windows_sys::core::GUID {
        data1: 0x886D_8EEB,
        data2: 0x8CF2,
        data3: 0x4446,
        data4: [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x4B],
    };

    /// windows-sys 未定义 IPropertyStore 接口，按 propsys.h 声明最小 vtbl
    #[repr(C)]
    #[derive(Clone, Copy)]
    #[allow(non_snake_case)]
    struct IPropertyStoreVtbl {
        QueryInterface: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *const windows_sys::core::GUID,
            *mut *mut core::ffi::c_void,
        ) -> windows_sys::core::HRESULT,
        AddRef: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
        Release: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
        GetCount: unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_sys::core::HRESULT,
        GetAt: unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut PROPERTYKEY) -> windows_sys::core::HRESULT,
        GetValue: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *const PROPERTYKEY,
            *mut PROPVARIANT,
        ) -> windows_sys::core::HRESULT,
        SetValue: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *const PROPERTYKEY,
            *const PROPVARIANT,
        ) -> windows_sys::core::HRESULT,
        Commit: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_sys::core::HRESULT,
    }

    /// AUMID 宽字符常驻（属性库可能持有指针，须与窗口同生命周期）
    fn aumid_wide() -> &'static [u16] {
        static WIDE: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
        WIDE.get_or_init(|| format!("{WINDOW_AUMID}\0").encode_utf16().collect())
    }

    /// 最近一次进程级 AUMID 设置结果（setup 时写入诊断日志）
    pub static PROCESS_AUMID_HR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

    /// 给窗口设独立 AUMID：官方机制 = 窗口属性库 IPropertyStore::SetValue；
    /// 另以 SetPropW 设两种已知属性名字符串兜底（无效名闲置无害）。
    unsafe fn set_window_aumid(hwnd: HWND) {
        let id = aumid_wide();

        let mut store: *mut core::ffi::c_void = std::ptr::null_mut();
        let hr = SHGetPropertyStoreForWindow(hwnd, &IID_IPROPERTYSTORE, &mut store);
        if hr == 0 && !store.is_null() {
            let vtbl = **(store as *mut *const IPropertyStoreVtbl);
            let mut pv: PROPVARIANT = core::mem::zeroed();
            pv.Anonymous.Anonymous = PROPVARIANT_0_0 {
                vt: 31, // VT_LPWSTR
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: id.as_ptr() as *mut u16,
                },
            };
            (vtbl.SetValue)(store, &PKEY_APP_USER_MODEL_ID, &pv);
            (vtbl.Commit)(store);
            (vtbl.Release)(store);
        }

        SetPropW(
            hwnd,
            windows_sys::core::w!("System.AppUserModel.ID"),
            id.as_ptr() as *mut core::ffi::c_void,
        );
        SetPropW(
            hwnd,
            windows_sys::core::w!("System.AppUserModelID"),
            id.as_ptr() as *mut core::ffi::c_void,
        );
    }

    /// RGBA 按目标尺寸 box-filter 缩放，输出 BGRA（32bpp 顶朝下 DIB 的内存字节序）
    fn scale_bgra(rgba: &[u8], sw: u32, sh: u32, tw: u32, th: u32) -> Vec<u8> {
        let mut out = vec![0u8; (tw * th * 4) as usize];
        for ty in 0..th {
            let y0 = ty * sh / th;
            let y1 = ((ty + 1) * sh / th).clamp(y0 + 1, sh);
            for tx in 0..tw {
                let x0 = tx * sw / tw;
                let x1 = ((tx + 1) * sw / tw).clamp(x0 + 1, sw);
                let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = ((y * sw + x) * 4) as usize;
                        r += rgba[i] as u32;
                        g += rgba[i + 1] as u32;
                        b += rgba[i + 2] as u32;
                        a += rgba[i + 3] as u32;
                        n += 1;
                    }
                }
                let o = ((ty * tw + tx) * 4) as usize;
                out[o] = (b / n) as u8;
                out[o + 1] = (g / n) as u8;
                out[o + 2] = (r / n) as u8;
                out[o + 3] = (a / n) as u8;
            }
        }
        out
    }

    /// BGRA 像素 → HICON（32bpp alpha 色位图 + 全零 1bpp mask，alpha 通道生效）
    unsafe fn make_hicon(bgra: &[u8], w: u32, h: u32) -> HICON {
        let dib = |bits: u16, height: i32| BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: height,
                biPlanes: 1,
                biBitCount: bits,
                biCompression: 0, // BI_RGB
                ..Default::default()
            },
            ..Default::default()
        };
        let dc = GetDC(std::ptr::null_mut());
        let mut color_bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(
            dc,
            &dib(32, -(h as i32)),
            0, // DIB_RGB_COLORS
            &mut color_bits,
            std::ptr::null_mut(),
            0,
        );
        let mut mask_bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let mask = CreateDIBSection(
            dc,
            &dib(1, h as i32),
            0,
            &mut mask_bits,
            std::ptr::null_mut(),
            0,
        );
        ReleaseDC(std::ptr::null_mut(), dc);
        if color.is_null() || mask.is_null() || color_bits.is_null() {
            return std::ptr::null_mut();
        }
        // CreateDIBSection 内存零初始化：mask 全零（不遮挡），色位图逐像素覆盖
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), color_bits as *mut u8, bgra.len());
        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let hicon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        hicon
    }

    /// 双通道设置窗口图标（small=标题栏/任务栏按钮，big=Alt-Tab），返回是否成功。
    /// 先脱钩 AUMID，再设类图标 + WM_SETICON；任务栏按钮（重）建时按
    /// "无快捷方式关联 → 窗口图标" 取值。
    pub unsafe fn apply(hwnd: HWND, rgba: &[u8], w: u32, h: u32) -> bool {
        set_window_aumid(hwnd);
        let (sw, sh) = (
            GetSystemMetrics(SM_CXSMICON).max(16) as u32,
            GetSystemMetrics(SM_CYSMICON).max(16) as u32,
        );
        let (bw, bh) = (
            GetSystemMetrics(SM_CXICON).max(16) as u32,
            GetSystemMetrics(SM_CYICON).max(16) as u32,
        );
        let small = make_hicon(&scale_bgra(rgba, w, h, sw, sh), sw, sh);
        let big = make_hicon(&scale_bgra(rgba, w, h, bw, bh), bw, bh);
        if small.is_null() || big.is_null() {
            return false;
        }
        SetClassLongPtrW(hwnd, GCLP_HICON, big as isize);
        SetClassLongPtrW(hwnd, GCLP_HICONSM, small as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small as isize);
        true
    }
}

/// 进程级 AUMID（须在任何窗口 show 之前调用）：显式 AUMID 使任务栏不再按
/// exe 路径把窗口关联到开始菜单快捷方式（快捷方式图标 = exe 内嵌图标），
/// 按钮图标回落为窗口图标。窗口级（属性库）版本实测未被图标选择采纳，
/// 进程级是 Electron/Qt 同款的可靠解耦机制。
#[cfg(windows)]
pub fn init_process_aumid() {
    let hr = unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            windows_sys::core::w!("com.zander.coding-plan-limit.main"),
        )
    };
    win_icon::PROCESS_AUMID_HR.store(hr as u32, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(not(windows))]
pub fn init_process_aumid() {}

/// 供 setup 写诊断日志
pub fn process_aumid_hr() -> u32 {
    #[cfg(windows)]
    {
        win_icon::PROCESS_AUMID_HR.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// 窗口显示前调用：任务栏按钮在窗口 show 时创建，创建时读取的类图标已是正确值
pub fn prime_window_icon(app: &AppHandle) {
    if let Some(img) = resolve_saved_logo(app) {
        apply_window_icon(app, &img);
    }
}

/// 运行中更换窗口图标后，Windows 11 任务栏按钮可能仍缓存旧图：
/// 主窗口可见时 hide → 短暂等待（让 explorer 销毁旧按钮）→ show 强制重建，
/// 重建时按钮读取的类图标已在 apply_window_icon 中更新。
/// Windows 专属——macOS 无任务栏按钮（窗口 hide/show 只会白闪），Linux 走窗口图标无需重建。
#[cfg(windows)]
fn refresh_taskbar_button(win: &tauri::WebviewWindow) {
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        std::thread::sleep(std::time::Duration::from_millis(60));
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 设置主窗口图标。
/// Windows：类图标 + WM_SETICON 双设（仅 WM_SETICON 时任务栏按钮不跟随），
/// 失败回落 tauri set_icon；
/// macOS：窗口无标题栏图标概念，set_icon 为无害空操作；Dock 图标 tauri 2
/// 未提供运行时 API（仅 dev 模式嵌入图），自定义 Logo 不跟随 Dock 属已知差距；
/// Linux（X11）：set_icon 即任务栏图标，正常生效。
pub fn apply_window_icon(app: &AppHandle, img: &tauri::image::Image<'_>) {
    if let Some(win) = app.get_webview_window("main") {
        #[cfg(windows)]
        {
            if let Ok(hwnd) = win.hwnd() {
                let ok =
                    unsafe { win_icon::apply(hwnd.0, img.rgba(), img.width(), img.height()) };
                debug_log(app, &format!("window_icon raw={ok}"));
                if ok {
                    refresh_taskbar_button(&win);
                    return;
                }
            }
            let _ = win.set_icon(img.clone());
            refresh_taskbar_button(&win);
        }
        #[cfg(not(windows))]
        {
            let _ = win.set_icon(img.clone());
        }
    }
}

/// macOS：Dock 图标跟随 Logo。tauri 2 无运行时 Dock 图标 API（仅 dev 模式设嵌入图），
/// 直调 NSApplication.setApplicationIconImage，须主线程（tauri 内部同款范式）。
/// NSImage 吃 PNG 编码字节，无需 PNG 编码器。
#[cfg(target_os = "macos")]
fn apply_dock_icon(app: &AppHandle, png: Vec<u8>) {
    let _ = app.run_on_main_thread(move || unsafe {
        use objc2::{AllocAnyThread, MainThreadMarker};
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let nsapp = NSApplication::sharedApplication(mtm);
        let data = NSData::with_bytes(&png);
        let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) else { return };
        nsapp.setApplicationIconImage(Some(&icon));
    });
}

/// 按样式同步 macOS Dock 图标（其他平台空操作）。
/// color=默认原色 / mark=Z 标 / custom=custom_icon 存档，三者均有现成 PNG 字节。
pub fn apply_dock_icon_for(app: &AppHandle, style: &str) {
    #[cfg(target_os = "macos")]
    {
        let png = match style {
            "custom" => store::load_settings(app)
                .custom_icon
                .as_deref()
                .and_then(decode_data_url_png_bytes),
            "mark" => Some(LOGO_MARK.to_vec()),
            _ => Some(DEFAULT_ICON_PNG.to_vec()),
        };
        if let Some(png) = png {
            apply_dock_icon(app, png);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, style);
    }
}

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
    debug_log(&app, "set_custom_icon: 解码成功，应用托盘+窗口图标");
    apply_tray_icon(&app, img.clone());
    // 同步主窗口标题栏/任务栏图标（任务栏 pinned 图标仍以 exe 内置图标为准，运行时无法替换）
    apply_window_icon(&app, &img);
    // macOS Dock 跟随（custom 存档此刻尚未落盘，直接用手上字节）
    #[cfg(target_os = "macos")]
    apply_dock_icon(&app, raw);

    store::update_config(&app, |config| {
        config.settings.custom_icon = Some(data_url);
        config.settings.logo_style = "custom".into();
    })?;
    Ok(())
}

/// 切换 Logo 样式：color（原色）| mark（Z 标）| custom（应用存档的自定义图）
#[tauri::command]
pub fn set_logo_style(app: AppHandle, style: String) -> Result<(), String> {
    // custom 不再是空操作：运行中从内置样式切回自定义时，需按存档图恢复托盘/窗口图标
    let custom_img: Option<tauri::image::Image> = if style == "custom" {
        let settings = store::load_settings(&app);
        Some(
            settings
                .custom_icon
                .and_then(|d| decode_data_url_png(&d))
                .ok_or("未配置自定义图标，请先选择图片")?,
        )
    } else {
        None
    };
    debug_log(&app, &format!("set_logo_style: {style}"));
    match style.as_str() {
        "color" => {
            let default_icon = app
                .default_window_icon()
                .expect("缺少应用图标")
                .clone();
            apply_tray_icon(&app, default_icon.clone());
            apply_window_icon(&app, custom_img.as_ref().unwrap_or(&default_icon));
        }
        "mark" => {
            let img = tauri::image::Image::from_bytes(LOGO_MARK)
                .map_err(|e| format!("Logo Mark 解析失败: {e}"))?;
            apply_tray_icon(&app, img.clone());
            apply_window_icon(&app, &img);
        }
        "custom" => {
            // style == "custom" 时上方已确保解码成功
            let img = custom_img.expect("custom 样式必有自定义图");
            apply_tray_icon(&app, img.clone());
            apply_window_icon(&app, &img);
        }
        _ => return Err(format!("未知 Logo 样式: {style}")),
    }
    // 自定义图片保留不清空：切换内置样式后仍可一键切回自定义
    store::update_config(&app, |config| {
        config.settings.logo_style = style.clone();
    })?;
    // macOS Dock 跟随三态（custom 读存档，此处必已落盘）
    apply_dock_icon_for(&app, &style);
    Ok(())
}

#[tauri::command]
pub fn reset_custom_icon(app: AppHandle) -> Result<(), String> {
    if let Some(default_icon) = app.default_window_icon() {
        let default_icon = default_icon.clone();
        // 托盘与主窗口（任务栏）同步恢复默认
        apply_tray_icon(&app, default_icon.clone());
        apply_window_icon(&app, &default_icon);
    }
    // macOS Dock 恢复默认原色
    apply_dock_icon_for(&app, "color");
    store::update_config(&app, |config| {
        config.settings.custom_icon = None;
        config.settings.logo_style = "color".into();
    })?;
    Ok(())
}

pub fn apply_tray_icon(app: &AppHandle, img: tauri::image::Image<'_>) {
    match app.tray_by_id("main-tray") {
        Some(tray) => {
            if let Err(e) = tray.set_icon(Some(img)) {
                debug_log(app, &format!("tray_icon 设置失败: {e}"));
            }
        }
        None => debug_log(app, "tray_icon: main-tray 不存在（托盘尚未创建）"),
    }
}

/// 删除套餐后单独推送视图（无需等刷新）
fn emit_views(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        scheduler::emit_views(&app).await;
    });
}
