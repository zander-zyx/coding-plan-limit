//! 小米 MiMo Token Plan：固定额度月用量
//! 原逻辑参考 claude-mini-hud usage.ts queryXiaomi（Cookie 优先，Bearer 兜底）。

use super::http::{f64_of, get_json};
use super::types::Quota;

pub async fn query(cookie: Option<&str>, bearer: Option<&str>) -> Result<Quota, String> {
    let url = "https://platform.xiaomimimo.com/api/v1/tokenPlan/usage";

    let mut headers: Vec<(&str, String)> = Vec::new();
    if let Some(c) = cookie.filter(|s| !s.trim().is_empty()) {
        headers.push(("Cookie", c.trim().to_string()));
    } else if let Some(k) = bearer.filter(|s| !s.trim().is_empty()) {
        headers.push(("Authorization", format!("Bearer {}", k.trim())));
    } else {
        return Err("需要 Cookie 或 API Key".into());
    }

    let json = get_json(url, &headers).await?;
    if json.get("code").and_then(|v| v.as_i64()) != Some(0) {
        return Err("接口返回 code != 0（Cookie 可能已过期）".into());
    }
    let usage = json
        .pointer("/data/monthUsage")
        .ok_or("响应缺少 data.monthUsage")?;

    // percent 为 0.0-1.0
    let used_percent = f64_of(usage, "percent")
        .map(|p| (p * 100.0).clamp(0.0, 100.0))
        .ok_or("缺少 percent 字段")?;

    let mut used = 0.0;
    let mut total = 0.0;
    if let Some(items) = usage.get("items").and_then(|v| v.as_array()) {
        for item in items {
            used += f64_of(item, "used").unwrap_or(0.0);
            total += f64_of(item, "limit").unwrap_or(0.0);
        }
    }

    Ok(Quota::FixedQuota {
        used_percent,
        used,
        total,
        unit: "tokens".into(),
        reset_at: None,
    })
}
