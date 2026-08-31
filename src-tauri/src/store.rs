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
    /// 已提醒过的新版本号（同一版本只无感提醒一次）
    #[serde(default)]
    pub notified_update: Option<String>,
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

/// 区分"文件不存在（给默认）"与"存在但损坏（中止写入保护数据）"
enum LoadOutcome {
    Loaded(Config),
    Missing,
    Corrupt(String),
}

fn load_config_full(app: &tauri::AppHandle) -> LoadOutcome {
    let path = match config_path(app) {
        Ok(p) => p,
        Err(_) => return LoadOutcome::Missing,
    };
    match fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(c) => LoadOutcome::Loaded(c),
            Err(e) => {
                // 损坏文件备份，便于用户手工恢复
                let _ = fs::copy(&path, path.with_extension("corrupt.bak"));
                LoadOutcome::Corrupt(e.to_string())
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LoadOutcome::Missing,
        Err(e) => LoadOutcome::Corrupt(e.to_string()),
    }
}

pub fn load_config(app: &tauri::AppHandle) -> Config {
    match load_config_full(app) {
        LoadOutcome::Loaded(c) => c,
        // 读取失败（权限等）与损坏同样返回默认，但 update_config 会拒绝写入，防覆盖
        _ => Config::default(),
    }
}

/// 私有写入（调用方需已持有 WRITE_LOCK）
fn write_config_unlocked(app: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    let path = config_path(app)?;
    atomic_write(&path, &serde_json::to_string_pretty(config).map_err(|e| e.to_string())?)
}

/// 加锁执行一次"读-改-写"，消除并发保存互相覆盖。
/// 配置损坏时中止写入（备份为 config.corrupt.bak），绝不把空配置写回。
pub fn update_config<F>(app: &tauri::AppHandle, f: F) -> Result<(), String>
where
    F: FnOnce(&mut Config),
{
    let guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut config = match load_config_full(app) {
        LoadOutcome::Loaded(c) => c,
        LoadOutcome::Missing => Config::default(),
        LoadOutcome::Corrupt(e) => {
            return Err(format!(
                "配置文件已损坏（备份为 config.corrupt.bak），为保护数据已中止写入: {e}"
            ));
        }
    };
    f(&mut config);
    let result = write_config_unlocked(app, &config);
    drop(guard);
    result
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

// ─── 本地加密文件后端（settings.secret_backend = "file"） ─────────────────
// secrets.bin：AES-256-GCM，密钥由固定盐 + 平台机器标识 + 用户名派生（绑定本机）。
// 动机：未签名应用在 macOS 每次升级都触发钥匙串授权弹窗，此 后端零弹窗。

const SECRETS_FILE: &str = "secrets.bin";
const SECRET_SLOTS: [&str; 7] = [
    "key",
    "cookie",
    "ak_id",
    "ak_secret",
    "codex_token",
    "codex_account",
    "codex_managed",
];

fn secrets_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join(SECRETS_FILE))
}

/// 平台机器标识：macOS IOPlatformUUID / Windows MachineGuid / Linux machine-id
fn machine_id() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(pos) = s.find("IOPlatformUUID") {
                if let Some(v) = s[pos..].split('"').nth(1) {
                    return v.to_string();
                }
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("reg")
            .args([
                "query",
                r"HKLM\SOFTWARE\Microsoft\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(v) = s.lines().last().and_then(|l| l.split_whitespace().nth(2)) {
                return v.to_string();
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = fs::read_to_string("/etc/machine-id") {
            return s.trim().to_string();
        }
    }
    "unknown-machine".to_string()
}

/// 加密主密钥：SHA256(固定盐 + 机器标识 + 用户名)，绑定本机，文件被拷走也无法解密
fn file_enc_key() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let mut h = Sha256::new();
    h.update(b"coding-plan-limit/secrets/v1");
    h.update(machine_id().as_bytes());
    h.update(user.as_bytes());
    h.finalize().into()
}

fn encrypt_b64(key: &[u8; 32], plain: &str) -> Result<String, String> {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("加密初始化失败: {e}"))?;
    // nonce 12B：取 UUID v4 的 12 个随机字节（v4 有 122 位随机性，每次写入唯一）
    let u = uuid::Uuid::new_v4();
    let nonce = Nonce::from_slice(&u.as_bytes()[4..16]);
    let ct = cipher
        .encrypt(nonce, plain.as_bytes())
        .map_err(|e| format!("加密失败: {e}"))?;
    let mut packed = Vec::with_capacity(12 + ct.len());
    packed.extend_from_slice(&u.as_bytes()[4..16]);
    packed.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(packed))
}

fn decrypt_b64(key: &[u8; 32], packed_b64: &str) -> Option<String> {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
    let packed = base64::engine::general_purpose::STANDARD.decode(packed_b64).ok()?;
    if packed.len() < 13 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let pt = cipher
        .decrypt(Nonce::from_slice(&packed[..12]), &packed[12..])
        .ok()?;
    String::from_utf8(pt).ok()
}

fn secrets_file_load(app: &tauri::AppHandle) -> HashMap<String, String> {
    let key = file_enc_key();
    secrets_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<HashMap<String, String>>(&s).ok())
        .map(|raw| {
            raw.into_iter()
                .filter_map(|(k, v)| decrypt_b64(&key, &v).map(|plain| (k, plain)))
                .collect()
        })
        .unwrap_or_default()
}

fn secrets_file_save(app: &tauri::AppHandle, map: &HashMap<String, String>) -> Result<(), String> {
    let key = file_enc_key();
    let raw: HashMap<String, String> = map
        .iter()
        .map(|(k, v)| encrypt_b64(&key, v).map(|e| (k.clone(), e)))
        .collect::<Result<_, _>>()?;
    let path = secrets_path(app)?;
    let json = serde_json::to_string(&raw).map_err(|e| format!("序列化失败: {e}"))?;
    atomic_write(&path, &json)
}

fn file_get(app: &tauri::AppHandle, k: &str) -> Option<String> {
    secrets_file_load(app).get(k).cloned()
}

fn file_set(app: &tauri::AppHandle, k: &str, v: &str) -> Result<(), String> {
    let mut map = secrets_file_load(app);
    map.insert(k.to_string(), v.to_string());
    secrets_file_save(app, &map)
}

fn file_remove(app: &tauri::AppHandle, k: &str) {
    let mut map = secrets_file_load(app);
    if map.remove(k).is_some() {
        let _ = secrets_file_save(app, &map);
    }
}

fn secret_backend_name(app: &tauri::AppHandle) -> String {
    let b = load_settings(app).secret_backend;
    if b == "file" {
        "file".to_string()
    } else {
        "keychain".to_string()
    }
}

/// 读密钥：凭据库优先，兜底配置文件。
/// 按模板注册表的 auth 类型分发，新增模板不再需要改这里。
pub fn load_credential(app: &tauri::AppHandle, plan: &PlanConfig) -> crate::usage::Credential {
    use crate::usage;
    let config = load_config(app);
    let mut cred = crate::usage::Credential::default();

    let take = |s: Option<String>| s.filter(|v| !v.is_empty());
    let auth = usage::templates()
        .into_iter()
        .find(|t| t.id == plan.template)
        .map(|t| t.auth)
        .unwrap_or_default();

    match auth.as_str() {
        "bearer" => cred.bearer = take(read_secret(app, &config, &plan.id, "key")),
        "cookie" => {
            cred.cookie = take(read_secret(app, &config, &plan.id, "cookie"));
            cred.bearer = take(read_secret(app, &config, &plan.id, "key"));
        }
        "bss" => {
            cred.ak_id = take(read_secret(app, &config, &plan.id, "ak_id"));
            cred.ak_secret = take(read_secret(app, &config, &plan.id, "ak_secret"));
        }
        // none：claude-official 无密钥；codex 多账号读捕获副本/托管绑定槽
        _ => {
            cred.codex_token = take(read_secret(app, &config, &plan.id, "codex_token"));
            cred.codex_account = take(read_secret(app, &config, &plan.id, "codex_account"));
            cred.codex_managed = take(read_secret(app, &config, &plan.id, "codex_managed"));
        }
    }
    cred
}

/// 查询前解析：托管账号绑定 → 换取有效 access_token（必要时刷新，轮换写回存储）
pub async fn resolve_managed_credential(
    app: &tauri::AppHandle,
    mut cred: crate::usage::Credential,
) -> Result<crate::usage::Credential, String> {
    if let Some(id) = cred.codex_managed.clone().filter(|s| !s.is_empty()) {
        let (token, ws) = crate::codex_oauth::valid_access(app, &id).await?;
        cred.codex_token = Some(token);
        cred.codex_account = Some(ws);
    }
    Ok(cred)
}

/// 删除单个密钥槽（凭据库 + 加密文件 + 明文兜底）
pub fn delete_secret_slot(app: &tauri::AppHandle, plan_id: &str, suffix: &str) {
    if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
        let _ = entry.delete_credential();
    }
    file_remove(app, &format!("{plan_id}:{suffix}"));
    let _ = update_config(app, |config| {
        config.fallback_secrets.remove(&format!("{plan_id}:{suffix}"));
    });
}

/// 直写明文兜底（静默）。超长密钥专用：Windows 凭据库单条上限 2560 个 UTF-16
/// 字符，Codex 的 access_token（大 JWT）必然超限，走 keyring 注定失败还弹警告。
pub fn save_secret_plain(
    app: &tauri::AppHandle,
    plan_id: &str,
    suffix: &str,
    value: &str,
) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    // 加密文件后端无长度限制，超长 JWT 也走加密存储
    if secret_backend_name(app) == "file" {
        let slot = format!("{plan_id}:{suffix}");
        file_set(app, &slot, value)?;
        let _ = update_config(app, |c| {
            c.fallback_secrets.remove(&slot);
        });
        return Ok(());
    }
    update_config(app, |config| {
        config
            .fallback_secrets
            .insert(format!("{plan_id}:{suffix}"), value.to_string());
    })
}

fn read_secret(app: &tauri::AppHandle, config: &Config, plan_id: &str, suffix: &str) -> Option<String> {
    let slot = format!("{plan_id}:{suffix}");
    // 1) 按当前后端优先读取
    if secret_backend_name(app) == "file" {
        if let Some(v) = file_get(app, &slot).filter(|v| !v.is_empty()) {
            return Some(v);
        }
    } else if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
        if let Ok(v) = entry.get_password() {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    // 2) 明文兜底
    if let Some(v) = config.fallback_secrets.get(&slot).filter(|v| !v.is_empty()) {
        return Some(v.clone());
    }
    // 3) 安全网：另一后端残留（切换中断时不丢密钥）
    if secret_backend_name(app) == "file" {
        if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
            if let Ok(v) = entry.get_password() {
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    } else {
        return file_get(app, &slot).filter(|v| !v.is_empty());
    }
    None
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
    // 本地加密文件后端：零弹窗
    if secret_backend_name(app) == "file" {
        let slot = format!("{plan_id}:{suffix}");
        file_set(app, &slot, value)?;
        let _ = update_config(app, |c| {
            c.fallback_secrets.remove(&slot);
        });
        if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
            let _ = entry.delete_credential();
        }
        return Ok(None);
    }
    match keyring_entry(&entry_user(plan_id, suffix)) {
        Ok(entry) => match entry.set_password(value) {
            Ok(()) => {
                // 密钥已入凭据库，清理历史降级明文（如有）
                let _ = update_config(app, |c| {
                    c.fallback_secrets.remove(&format!("{plan_id}:{suffix}"));
                });
                return Ok(None);
            }
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
    for suffix in SECRET_SLOTS {
        if let Ok(entry) = keyring_entry(&entry_user(plan_id, suffix)) {
            let _ = entry.delete_credential();
        }
        file_remove(app, &format!("{plan_id}:{suffix}"));
    }
    let _ = update_config(app, |config| {
        config
            .fallback_secrets
            .retain(|k, _| !k.starts_with(&format!("{plan_id}:")));
    });
}

/// 切换密钥存储后端并把现有密钥迁到新后端。返回迁移摘要。
/// 迁移期间 keychain→file 需要读钥匙串，macOS 可能弹一次授权（选"始终允许"）。
pub fn set_secret_backend(app: &tauri::AppHandle, backend: &str) -> Result<String, String> {
    let backend = backend.trim();
    if backend != "keychain" && backend != "file" {
        return Err(format!("未知存储方式: {backend}"));
    }
    if secret_backend_name(app) == backend {
        return Ok("当前已是该存储方式".to_string());
    }

    // 先落标记：读写从此走新后端；迁移读旧后端显式进行
    update_config(app, |c| c.settings.secret_backend = backend.to_string())?;

    let plans = load_plans(app);
    let mut moved = 0usize;
    let mut warned: Vec<String> = Vec::new();

    for plan in &plans {
        for slot in SECRET_SLOTS {
            let slot_key = format!("{}:{}", plan.id, slot);
            if backend == "file" {
                // keychain + 明文兜底 → 加密文件
                let mut value = file_get(app, &slot_key).filter(|v| !v.is_empty());
                if value.is_none() {
                    if let Ok(entry) = keyring_entry(&entry_user(&plan.id, slot)) {
                        if let Ok(v) = entry.get_password() {
                            if !v.is_empty() {
                                value = Some(v);
                            }
                        }
                    }
                }
                if let Some(v) = value {
                    file_set(app, &slot_key, &v)?;
                    if let Ok(entry) = keyring_entry(&entry_user(&plan.id, slot)) {
                        let _ = entry.delete_credential();
                    }
                    let _ = update_config(app, |c| {
                        c.fallback_secrets.remove(&slot_key);
                    });
                    moved += 1;
                }
            } else {
                // 加密文件 + 明文兜底 → keychain
                let mut value = file_get(app, &slot_key).filter(|v| !v.is_empty());
                if value.is_none() {
                    if let Some(v) = load_config(app).fallback_secrets.get(&slot_key).cloned() {
                        if !v.is_empty() {
                            value = Some(v);
                        }
                    }
                }
                if let Some(v) = value {
                    match keyring_entry(&entry_user(&plan.id, slot))
                        .and_then(|e| e.set_password(&v).map_err(|e| e.to_string()))
                    {
                        Ok(()) => {
                            file_remove(app, &slot_key);
                            let _ = update_config(app, |c| {
                                c.fallback_secrets.remove(&slot_key);
                            });
                            moved += 1;
                        }
                        Err(e) => {
                            // 超长 JWT 等 keyring 存不下的：留在明文兜底并提示
                            warned.push(format!("{}:{}", plan.name, slot));
                            let _ = persist_fallback(app, &plan.id, slot, &v);
                            file_remove(app, &slot_key);
                            let _ = e;
                        }
                    }
                }
            }
        }
    }

    let target = if backend == "file" { "本地加密文件" } else { "系统凭据库" };
    let mut summary = format!("已切换到{target}，迁移 {moved} 条密钥");
    if !warned.is_empty() {
        summary.push_str(&format!(
            "；{} 条超长密钥无法入凭据库，已保留明文兜底",
            warned.len()
        ));
    }
    Ok(summary)
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
