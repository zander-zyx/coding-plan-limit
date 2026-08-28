//! 配置持久化：config.json（套餐 + 设置 + 凭据库不可用时的兜底）
//! 密钥优先存系统凭据库（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::usage::types::{PlanConfig, Settings};

const KEYRING_SERVICE: &str = "coding-plan-limit";
const CONFIG_FILE: &str = "config.json";
const SNAPSHOTS_FILE: &str = "snapshots.json";

static WRITE_LOCK: Mutex<()> = Mutex::new(());

// ─── 配置文件结构 ──────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub plans: Vec<PlanConfig>,
    #[serde(default)]
    pub settings: Settings,
    /// 系统凭据库不可用时的明文兜底（启动/保存时会向用户提示风险）
    #[serde(default)]
    pub fallback_secrets: HashMap<String, String>,
}

// ─── 路径 ─────────────────────────────────────────────────────────────────

pub fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    Ok(dir)
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join(CONFIG_FILE))
}

pub fn snapshots_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join(SNAPSHOTS_FILE))
}

// ─── 读写 ─────────────────────────────────────────────────────────────────

pub fn load_config(app: &tauri::AppHandle) -> Config {
    config_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 私有写入（调用方需已持有 WRITE_LOCK）
fn write_config_unlocked(app: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    let path = config_path(app)?;
    atomic_write(&path, &serde_json::to_string_pretty(config).map_err(|e| e.to_string())?)
}

/// 加锁执行一次"读-改-写"，消除并发保存互相覆盖
pub fn update_config<F>(app: &tauri::AppHandle, f: F) -> Result<(), String>
where
    F: FnOnce(&mut Config),
{
    let _guard = WRITE_LOCK.lock().map_err(|_| "配置写入锁异常")?;
    let mut config = load_config(app);
    f(&mut config);
    write_config_unlocked(app, &config)
}

pub fn load_plans(app: &tauri::AppHandle) -> Vec<PlanConfig> {
    load_config(app).plans
}

pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    load_config(app).settings
}

// ─── 凭据 ─────────────────────────────────────────────────────────────────

fn keyring_entry(plan_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("plan/{plan_id}"))
        .map_err(|e| format!("凭据库不可用: {e}"))
}

/// 读密钥：凭据库优先，兜底配置文件
pub fn load_credential(app: &tauri::AppHandle, plan: &PlanConfig) -> crate::usage::Credential {
    let config = load_config(app);
    let mut cred = crate::usage::Credential::default();

    let take = |s: Option<String>| s.filter(|v| !v.is_empty());

    // 单密钥模板：凭据库一条 set_password
    if matches!(
        plan.template.as_str(),
        "minimax" | "zhipu" | "kimi-coding" | "deepseek" | "kimi" | "stepfun" | "siliconflow"
    ) {
        cred.bearer = take(read_secret(app, &config, &plan.id, "key"));
    } else if plan.template == "xiaomi" {
        cred.cookie = take(read_secret(app, &config, &plan.id, "cookie"));
        cred.bearer = take(read_secret(app, &config, &plan.id, "key"));
    } else if plan.template == "alibaba" {
        cred.ak_id = take(read_secret(app, &config, &plan.id, "ak_id"));
        cred.ak_secret = take(read_secret(app, &config, &plan.id, "ak_secret"));
    }
    cred
}

fn read_secret(_app: &tauri::AppHandle, config: &Config, plan_id: &str, suffix: &str) -> Option<String> {
    // 1) 系统凭据库
    if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
        if let Ok(v) = entry.get_password() {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    // 2) 明文兜底
    config.fallback_secrets.get(&format!("{plan_id}:{suffix}")).cloned()
}

fn entry_user(plan_id: &str, suffix: &str) -> String {
    if suffix == "key" {
        format!("plan/{plan_id}")
    } else {
        format!("plan/{plan_id}/{suffix}")
    }
}

/// 写密钥：凭据库失败时落明文兜底，返回警告信息
pub fn save_secret(
    app: &tauri::AppHandle,
    plan_id: &str,
    suffix: &str,
    value: &str,
) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    match keyring_entry(&entry_user(plan_id, suffix)) {
        Ok(entry) => match entry.set_password(value) {
            Ok(()) => return Ok(None),
            Err(e) => {
                let warn = format!("系统凭据库写入失败（{e}），密钥将明文保存在本地配置文件中");
                persist_fallback(app, plan_id, suffix, value)?;
                return Ok(Some(warn));
            }
        },
        Err(_) => {
            let warn = "系统凭据库不可用，密钥将明文保存在本地配置文件中".to_string();
            persist_fallback(app, plan_id, suffix, value)?;
            return Ok(Some(warn));
        }
    }
}

pub fn delete_secret(app: &tauri::AppHandle, plan_id: &str) {
    for suffix in ["key", "cookie", "ak_id", "ak_secret"] {
        if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
            let _ = entry.delete_credential();
        }
    }
    let _ = update_config(app, |config| {
        config
            .fallback_secrets
            .retain(|k, _| !k.starts_with(&format!("{plan_id}:")));
    });
}

fn persist_fallback(app: &tauri::AppHandle, plan_id: &str, suffix: &str, value: &str) -> Result<(), String> {
    update_config(app, |config| {
        config
            .fallback_secrets
            .insert(format!("{plan_id}:{suffix}"), value.to_string());
    })
}

// ─── 快照缓存 ─────────────────────────────────────────────────────────────

pub fn load_snapshots(
    app: &tauri::AppHandle,
) -> HashMap<String, crate::usage::types::Snapshot> {
    snapshots_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_snapshots(
    app: &tauri::AppHandle,
    snapshots: &HashMap<String, crate::usage::types::Snapshot>,
) {
    if let Ok(path) = snapshots_path(app) {
        if let Ok(json) = serde_json::to_string_pretty(snapshots) {
            let _ = atomic_write(&path, &json);
        }
    }
}

// ─── 原子写：写临时文件后原子替换，杜绝"目标文件短暂不存在"的空窗 ─────────

fn atomic_write(path: &PathBuf, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| format!("写入失败: {e}"))?;
    replace_file(&tmp, path)
}

#[cfg(windows)]
fn replace_file(tmp: &PathBuf, path: &PathBuf) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    fn to_wide(p: &PathBuf) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            to_wide(tmp).as_ptr(),
            to_wide(path).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err("替换配置失败（MoveFileExW）".into())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(tmp: &PathBuf, path: &PathBuf) -> Result<(), String> {
    fs::rename(tmp, path).map_err(|e| format!("替换配置失败: {e}"))
}
