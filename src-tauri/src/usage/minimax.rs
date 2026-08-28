//! MiniMax Coding Plan：剩余 token 百分比（5h / 周 / 月）
//! 原逻辑参考 claude-mini-hud usage.ts queryMiniMax。

use super::http::{f64_of, get_json, ms_to_secs};
use super::types::{Quota, WindowQuota};

pub async fn query(region: &str, bearer: &str) -> Result<Quota, String> {
    let host = if region == "intl" {
        "https://api.minimax.io"
    } else {
        "https://api.minimaxi.com"
    };
    let url = format!("{host}/v1/api/openplatform/coding_plan/remains");
    let json = get_json(
        &url,
        &[("Authorization", format!("Bearer {bearer}"))],
    )
    .await?;

    let list = json
        .get("model_remains")
        .and_then(|v| v.as_array())
        .ok_or("响应缺少 model_remains")?;
    if list.is_empty() {
        return Err("model_remains 为空".into());
    }
    // 与原逻辑一致：优先 general（Coding Plan 默认模型），否则第一条
    let main = list
        .iter()
        .find(|m| m.get("model_name").and_then(|v| v.as_str()) == Some("general"))
        .unwrap_or_else(|| &list[0]);

    let mut windows = Vec::new();
    let mut push = |label: &str, remain_key: &str, reset_key: &str| {
        if let Some(remain) = f64_of(main, remain_key) {
            windows.push(WindowQuota {
                label: label.to_string(),
                used_percent: (100.0 - remain).clamp(0.0, 100.0),
                reset_at: ms_to_secs(f64_of(main, reset_key)),
            });
        }
    };
    push("5小时", "current_interval_remaining_percent", "end_time");
    push("本周", "current_weekly_remaining_percent", "weekly_end_time");
    push("本月", "current_monthly_remaining_percent", "monthly_end_time");

    if windows.is_empty() {
        return Err("响应中无用量数据".into());
    }
    Ok(Quota::Windows { windows })
}
