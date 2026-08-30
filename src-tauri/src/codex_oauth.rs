//! Codex/ChatGPT 多账号自管：OAuth Device Code 登录 + refresh token 轮换刷新。
//! 流程与端点对齐 cc-switch codex_oauth_auth.rs（client_id 与官方 Codex CLI 相同）。
//!
//! ⚠ ChatGPT 的 refresh token 是轮换型：谁刷新谁拿走会话。本模块只刷新自己
//! 存储里的账号；用户若在 CC Switch 等工具登录了同一账号，两边会互相挤掉，
//! 这是上游机制，非缺陷。捕获式绑定（只读 ~/.codex/auth.json 副本）无此冲突。
//!
//! 存储文件：<config_dir>/codex_oauth.json（version 1）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::AppHandle;

use crate::usage::http::client;

const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const USERCODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
const REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

// ─── 存储 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedAccount {
    /// 上游 ChatGPT workspace ID（同时作为本地稳定 ID）
    pub account_id: String,
    #[serde(default)]
    pub email: Option<String>,
    pub refresh_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    /// access_token 过期时刻（毫秒）；0 = 无缓存
    #[serde(default)]
    pub access_expires_at_ms: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    version: u32,
    accounts: BTreeMap<String, ManagedAccount>,
}

fn store_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = crate::store::config_dir(app)?;
    Ok(dir.join("codex_oauth.json"))
}

fn load(app: &AppHandle) -> Store {
    std::fs::read_to_string(store_path(app).unwrap_or_default())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Store {
            version: 1,
            accounts: BTreeMap::new(),
        })
}

fn save(app: &AppHandle, store: &Store) -> Result<(), String> {
    let path = store_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("写入账号存储失败: {e}"))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 账号列表（不含任何 token，供前端展示）
pub fn list_accounts(app: &AppHandle) -> Vec<ManagedAccount> {
    let mut v: Vec<_> = load(app).accounts.into_values().collect();
    v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    for a in &mut v {
        a.refresh_token = String::new();
        a.access_token = None;
        a.id_token = a.id_token.as_ref().map(|_| String::new());
    }
    v
}

pub fn delete_account(app: &AppHandle, account_id: &str) -> Result<(), String> {
    let mut store = load(app);
    if store.accounts.remove(account_id).is_none() {
        return Err(format!("账号不存在: {account_id}"));
    }
    save(app, &store)
}

// ─── JWT claims ───────────────────────────────────────────────────────────

/// 解析 id_token（JWT）payload 的 chatgpt_account_id / email
fn jwt_metadata(id_token: &str) -> (Option<String>, Option<String>) {
    let parse = || -> Option<(Option<String>, Option<String>)> {
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        use base64::Engine as _;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
        let json: serde_json::Value = serde_json::from_slice(&payload).ok()?;
        let claim = |key: &str| {
            json.get(key)
                .and_then(|v| v.as_str())
                .map(String::from)
                .filter(|s| !s.is_empty())
        };
        let account = claim("chatgpt_account_id").or_else(|| {
            json.pointer("/https://api.openai.com/auth/chatgpt_account_id")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        Some((account, claim("email")))
    };
    parse().unwrap_or((None, None))
}

// ─── OAuth 请求 ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

async fn oauth_form(form: &[(&str, &str)]) -> Result<TokenResponse, String> {
    let body = urlencode(form);
    let resp = client()
        .post(OAUTH_TOKEN_URL)
        .timeout(std::time::Duration::from_secs(30))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        // refresh_token 失效：上游会返回明确错误码
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            let code = v
                .pointer("/error/code")
                .or_else(|| v.get("error"))
                .and_then(|c| {
                    c.as_str()
                        .map(String::from)
                        .or_else(|| c.get("code").and_then(|x| x.as_str()).map(String::from))
                })
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(
                code.as_str(),
                "refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated"
            ) || status.as_u16() == 401
            {
                return Err("凭据已失效，请重新登录该账号".into());
            }
        }
        let brief: String = text.chars().take(160).collect();
        return Err(format!("OAuth 请求失败: HTTP {} {brief}", status.as_u16()));
    }
    serde_json::from_str(&text).map_err(|e| format!("OAuth 响应解析失败: {e}"))
}

/// application/x-www-form-urlencoded（RFC 3986 非保留字符外的全部百分号编码）
fn urlencode(form: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for b in s.as_bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(*b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    };
    form.iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

// ─── Device Code 登录 ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LoginStart {
    pub user_code: String,
    pub verification_url: &'static str,
    pub device_code: String,
    pub expires_in: u64,
}

pub async fn login_start() -> Result<LoginStart, String> {
    #[derive(Deserialize)]
    struct Resp {
        device_auth_id: String,
        user_code: String,
    }
    let resp = client()
        .post(USERCODE_URL)
        .timeout(std::time::Duration::from_secs(30))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&serde_json::json!({ "client_id": CODEX_CLIENT_ID }))
            .map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("启动登录失败: HTTP {}", status.as_u16()));
    }
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    let r: Resp = serde_json::from_str(&text).map_err(|e| format!("响应解析失败: {e}"))?;
    Ok(LoginStart {
        user_code: r.user_code,
        verification_url: VERIFICATION_URL,
        device_code: r.device_auth_id,
        expires_in: 900,
    })
}

/// 轮询一次：Ok(None) = 用户尚未授权；Ok(Some(acc)) = 登录成功（已入库）
pub async fn login_poll(
    app: &AppHandle,
    device_code: &str,
    user_code: &str,
) -> Result<Option<ManagedAccount>, String> {
    #[derive(Deserialize)]
    struct Resp {
        authorization_code: String,
        code_verifier: String,
    }
    let resp = client()
        .post(DEVICE_TOKEN_URL)
        .timeout(std::time::Duration::from_secs(30))
        .header("Content-Type", "application/json")
        .body(
            serde_json::to_string(&serde_json::json!({
                "device_auth_id": device_code,
                "user_code": user_code,
            }))
            .map_err(|e| e.to_string())?,
        )
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status().as_u16();
    if status == 403 || status == 404 {
        return Ok(None); // 等待用户授权
    }
    if status == 410 {
        return Err("登录码已过期，请重新发起登录".into());
    }
    if !(200..300).contains(&status) {
        return Err(format!("轮询登录状态失败: HTTP {status}"));
    }
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    let r: Resp = serde_json::from_str(&text).map_err(|e| format!("响应解析失败: {e}"))?;

    let tokens = oauth_form(&[
        ("grant_type", "authorization_code"),
        ("code", r.authorization_code.as_str()),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", CODEX_CLIENT_ID),
        ("code_verifier", r.code_verifier.as_str()),
    ])
    .await?;
    let refresh = tokens
        .refresh_token
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or("登录响应缺少 refresh_token，账号未保存")?;
    let (account_id, email) = tokens
        .id_token
        .as_deref()
        .map(jwt_metadata)
        .unwrap_or((None, None));
    let account_id = account_id.ok_or("无法从登录响应确认账号身份，账号未保存")?;

    let acc = ManagedAccount {
        account_id: account_id.clone(),
        email,
        refresh_token: refresh,
        id_token: tokens.id_token.clone(),
        access_token: Some(tokens.access_token),
        access_expires_at_ms: now_ms() + (tokens.expires_in.unwrap_or(3600).max(60) as i64) * 1000,
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    };
    let mut store = load(app);
    store.accounts.insert(account_id, acc.clone());
    save(app, &store)?;
    Ok(Some(acc))
}

// ─── 取有效 access_token（必要时刷新，轮换写回存储） ─────────────────────

pub async fn valid_access(app: &AppHandle, account_id: &str) -> Result<(String, String), String> {
    let mut store = load(app);
    let acc = store
        .accounts
        .get(account_id)
        .ok_or_else(|| format!("账号不存在: {account_id}"))?
        .clone();

    if let Some(tok) = &acc.access_token {
        if acc.access_expires_at_ms - 60_000 > now_ms() {
            return Ok((tok.clone(), acc.account_id));
        }
    }

    let tokens = oauth_form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", acc.refresh_token.as_str()),
        ("client_id", CODEX_CLIENT_ID),
        ("scope", "openid profile email"),
    ])
    .await?;

    let mut acc = acc;
    acc.access_token = Some(tokens.access_token);
    acc.access_expires_at_ms =
        now_ms() + (tokens.expires_in.unwrap_or(3600).max(60) as i64) * 1000;
    if let Some(rt) = tokens.refresh_token.filter(|s| !s.is_empty()) {
        acc.refresh_token = rt; // 轮换：写回新 refresh_token
    }
    if let Some(idt) = tokens.id_token {
        acc.id_token = Some(idt);
    }
    acc.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let ws = acc.account_id.clone();
    store.accounts.insert(account_id.to_string(), acc);
    save(app, &store)?;
    Ok((store.accounts[account_id].access_token.clone().unwrap_or_default(), ws))
}

// ─── 捕获式：读取本机 ~/.codex/auth.json ─────────────────────────────────

/// 读当前 Codex CLI 登录凭据（不刷新）；供「捕获当前登录」存为套餐私有副本
pub fn capture_current() -> Result<(String, Option<String>), String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "无法定位用户主目录".to_string())?;
    let path = std::path::Path::new(&home).join(".codex").join("auth.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| "未找到 Codex CLI 凭据（请先 codex login）".to_string())?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("凭据解析失败: {e}"))?;
    let token = json
        .pointer("/tokens/access_token")
        .or_else(|| json.get("access_token"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("凭据中无 access_token")?
        .to_string();
    let account = json
        .pointer("/tokens/account_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok((token, account))
}
