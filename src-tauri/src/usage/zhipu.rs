//! 智谱 GLM Coding Plan：5h / 周 / 月 / MCP 用量百分比
//! 原逻辑参考 claude-mini-hud usage.ts queryZhipu。

use super::http::{f64_of, get_json, ms_to_secs};
use super::types::{Quota, WindowQuota};

/// 从用户 base_url（或区域默认）推导监控端点：
/// 去掉 /anthropic 后缀 + 拼 /monitor/usage/quota/limit
fn resolve_base(region: &str, base_url: Option<&str>) -> String {
    let raw = base_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if region == "intl" {
                "https://api.z.ai".to_string()
            } else {
                "https://open.bigmodel.cn".to_string()
            }
        });
    let trimmed = raw.trim_end_matches('/');
    match trimmed.strip_suffix("/anthropic") {
        Some(stripped) => stripped.to_string(),
        None => trimmed.to_string(),
    }
}

pub async fn query(region: &str, base_url: Option<&str>, bearer: &str) -> Result<Quota, String> {
    let url = format!("{}/monitor/usage/quota/limit", resolve_base(region, base_url));
    let json = get_json(&url, &[("Authorization", format!("Bearer {bearer}"))]).await?;

    if json.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return Err("接口返回 success=false".into());
    }
    let data = json.get("data").ok_or("响应缺少 data")?;
    let limits = data
        .get("limits")
        .and_then(|v| v.as_array())
        .ok_or("响应缺少 data.limits")?;

    // 2026-08 起智谱迁积分制：TOKENS_LIMIT / CREDIT_LIMIT 均视为额度项
    let is_quota = |t: &str| t == "TOKENS_LIMIT" || t == "CREDIT_LIMIT";

    let mut windows: Vec<WindowQuota> = Vec::new();
    let mut monthly_done = false;

    for item in limits {
        let typ = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let unit = f64_of(item, "unit").unwrap_or(0.0) as i64;
        let Some(pct) = f64_of(item, "percentage") else {
            continue;
        };
        let reset = ms_to_secs(f64_of(item, "nextResetTime"));
        if is_quota(typ) && unit == 3 {
            windows.push(WindowQuota { label: "5小时".into(), used_percent: pct, reset_at: reset });
        } else if is_quota(typ) && unit == 6 {
            windows.push(WindowQuota { label: "本周".into(), used_percent: pct, reset_at: reset });
        } else if typ == "TIME_LIMIT" {
            windows.push(WindowQuota { label: "MCP".into(), used_percent: pct, reset_at: None });
        } else if is_quota(typ) && !monthly_done {
            windows.push(WindowQuota { label: "本月".into(), used_percent: pct, reset_at: reset });
            monthly_done = true;
        }
    }

    if windows.is_empty() {
        return Err("响应中无用量数据".into());
    }
    Ok(Quota::Windows { windows })
}
