//! Claude Official / Codex（ChatGPT）官方订阅额度查询。
//! 凭据来自本机 CLI 登录文件（只读，不刷新 token，过期时提示重新登录）：
//!   Claude: ~/.claude/.credentials.json → claudeAiOauth.accessToken
//!   Codex : ~/.codex/auth.json → tokens.access_token (+ account_id)
//!
//! 实现对齐 cc-switch services/subscription.rs：
//!   Claude: GET https://api.anthropic.com/api/oauth/usage（beta: oauth-2025-04-20）
//!   Codex : GET https://chatgpt.com/backend-api/wham/usage

use serde_json::Value;

use super::types::{Quota, WindowQuota};

fn home_path(rel: &[&str]) -> Option<std::path::PathBuf> {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok()?;
    let mut p = std::path::PathBuf::from(home);
    for seg in rel {
        p.push(seg);
    }
    Some(p)
}

/// resets_at 兼容 unix 秒 / 毫秒 / ISO8601 字符串
fn parse_reset(v: Option<&Value>) -> Option<i64> {
    let v = v?;
    let secs = |n: i64| if n > 10_000_000_000 { n / 1000 } else { n };
    if let Some(n) = v.as_i64() {
        return Some(secs(n));
    }
    if let Some(n) = v.as_f64() {
        return Some(secs(n as i64));
    }
    let s = v.as_str()?;
    if let Ok(ts) = s.parse::<i64>() {
        return Some(secs(ts));
    }
    // ISO8601
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

/// 统计窗口秒数 → 中文标签
fn window_label(secs: Option<i64>) -> String {
    match secs {
        Some(18_000) => "5小时".into(),
        Some(604_800) => "7天".into(),
        Some(2_592_000) => "30天".into(),
        Some(s) if s >= 86_400 => format!("{}天", s / 86_400),
        Some(s) if s >= 3_600 => format!("{}小时", s / 3_600),
        _ => "窗口".into(),
    }
}

// ─── Claude Official ──────────────────────────────────────────────────────

fn read_claude_token() -> Result<String, String> {
    let path = home_path(&[".claude", ".credentials.json"])
        .ok_or("无法定位用户主目录")?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| "未找到 Claude CLI 凭据（请先 claude login / 设置 API Key）".to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("凭据解析失败: {e}"))?;
    let token = json
        .pointer("/claudeAiOauth/accessToken")
        .or_else(|| json.pointer("/claude.ai_oauth/accessToken"))
        .or_else(|| json.pointer("/oauthAccount/accessToken"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("凭据中无 accessToken（可能使用 API Key 登录）")?;
    Ok(token.to_string())
}

pub async fn claude() -> Result<Quota, String> {
    let token = read_claude_token()?;
    let json = super::http::get_json(
        "https://api.anthropic.com/api/oauth/usage",
        &[
            ("Authorization", format!("Bearer {token}")),
            ("anthropic-beta", "oauth-2025-04-20".into()),
        ],
    )
    .await?;

    let mut windows = Vec::new();
    for key in ["five_hour", "seven_day", "seven_day_opus", "seven_day_sonnet"] {
        let Some(w) = json.get(key) else { continue };
        let Some(util) = w.get("utilization").and_then(|v| v.as_f64()) else {
            continue;
        };
        windows.push(WindowQuota {
            label: window_label(match key {
                "five_hour" => Some(18_000),
                _ => Some(604_800),
            }),
            used_percent: util.clamp(0.0, 100.0),
            reset_at: parse_reset(w.get("resets_at")),
        });
    }
    // 未知的窗口类型也收（API 可能新增）
    if let Some(obj) = json.as_object() {
        for (k, v) in obj {
            if ["extra_usage", "five_hour", "seven_day", "seven_day_opus", "seven_day_sonnet"]
                .contains(&k.as_str())
            {
                continue;
            }
            if let Some(util) = v.get("utilization").and_then(|x| x.as_f64()) {
                windows.push(WindowQuota {
                    label: k.clone(),
                    used_percent: util.clamp(0.0, 100.0),
                    reset_at: parse_reset(v.get("resets_at")),
                });
            }
        }
    }

    if windows.is_empty() {
        return Err("暂不支持：接口未返回窗口额度数据".into());
    }
    Ok(Quota::Windows { windows })
}

// ─── Codex / ChatGPT ──────────────────────────────────────────────────────

fn read_codex_token() -> Result<(String, Option<String>), String> {
    let path = home_path(&[".codex", "auth.json"]).ok_or("无法定位用户主目录")?;
    let text =
        std::fs::read_to_string(&path).map_err(|_| "未找到 Codex CLI 凭据（请先 codex login）".to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("凭据解析失败: {e}"))?;
    let token = json
        .pointer("/tokens/access_token")
        .or_else(|| json.get("access_token"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("凭据中无 access_token")?
        .to_string();
    let account = json
        .pointer("/tokens/account_id")
        .or_else(|| json.get("account_id"))
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok((token, account))
}

pub async fn codex() -> Result<Quota, String> {
    let (token, account) = read_codex_token()?;
    let mut headers = vec![
        ("Authorization", format!("Bearer {token}")),
        ("User-Agent", "codex-cli".to_string()),
    ];
    if let Some(id) = account {
        headers.push(("ChatGPT-Account-Id", id));
    }
    let json = super::http::get_json("https://chatgpt.com/backend-api/wham/usage", &headers).await?;

    let mut windows = Vec::new();
    let limit = json.get("rate_limit");
    for key in ["primary_window", "secondary_window"] {
        let Some(w) = limit.and_then(|l| l.get(key)) else { continue };
        let Some(used) = w.get("used_percent").and_then(|v| v.as_f64()) else {
            continue;
        };
        let secs = w.get("limit_window_seconds").and_then(|v| v.as_i64());
        windows.push(WindowQuota {
            label: window_label(secs),
            used_percent: used.clamp(0.0, 100.0),
            reset_at: parse_reset(w.get("reset_at")),
        });
    }

    if windows.is_empty() {
        return Err("暂不支持：接口未返回窗口额度数据".into());
    }
    Ok(Quota::Windows { windows })
}
