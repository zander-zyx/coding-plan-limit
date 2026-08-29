//! Claude 原生套餐：读取 claude-mini-hud 的本地缓存（~/.claude-mini-hud/usage-cache/claude.json）。
//! Claude 官方不提供独立额度 API，rate_limits 只存在于 Claude Code 会话 stdin 中；
//! 若用户同时运行 claude-mini-hud，这里复用其缓存数据。

use std::path::PathBuf;

use serde_json::Value;

use super::types::{Quota, WindowQuota};

fn cache_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(
        PathBuf::from(home)
            .join(".claude-mini-hud")
            .join("usage-cache")
            .join("claude.json"),
    )
}

pub async fn query() -> Result<Quota, String> {
    let path = cache_path().ok_or("无法定位用户主目录")?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| "未找到 claude-mini-hud 缓存（需运行 claude-mini-hud 生成）".to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("缓存解析失败: {e}"))?;

    let claude = json.get("claude").ok_or("缓存缺少 claude 字段")?;
    let mut windows = Vec::new();
    let mut push = |label: &str, key: &str, reset_key: &str| {
        if let Some(pct) = claude.get(key).and_then(|v| v.as_f64()) {
            windows.push(WindowQuota {
                label: label.to_string(),
                used_percent: pct.clamp(0.0, 100.0),
                reset_at: claude.get(reset_key).and_then(|v| v.as_i64()),
            });
        }
    };
    push("5小时", "fiveHour", "fiveHourResetAt");
    push("7天", "sevenDay", "sevenDayResetAt");

    if windows.is_empty() {
        return Err("缓存中无 rate_limits 数据".into());
    }
    Ok(Quota::Windows { windows })
}
