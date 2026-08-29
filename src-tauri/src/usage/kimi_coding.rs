//! Kimi For Coding 用量：5小时窗口 + 周限额
//! 与 claude-mini-hud usage.ts 一致：limits 中最后一个带 detail 的项生效（覆写语义）。

use super::http::{f64_of, get_json, ms_to_secs};
use super::types::{Quota, WindowQuota};

pub async fn query(bearer: &str) -> Result<Quota, String> {
    let url = "https://api.kimi.com/coding/v1/usages";
    let json = get_json(url, &[("Authorization", format!("Bearer {bearer}"))]).await?;

    let mut windows: Vec<WindowQuota> = Vec::new();

    // 5 小时窗口：limits[] 中最后一个带 detail 的项生效
    if let Some(limits) = json.get("limits").and_then(|v| v.as_array()) {
        if let Some(detail) = limits.iter().rev().find_map(|i| i.get("detail")) {
            let limit = f64_of(detail, "limit").unwrap_or(1.0);
            let remaining = f64_of(detail, "remaining").unwrap_or(0.0);
            let used = (limit - remaining).max(0.0);
            let pct = if limit > 0.0 { used / limit * 100.0 } else { 0.0 };
            windows.push(WindowQuota {
                label: "5小时".into(),
                used_percent: pct.clamp(0.0, 100.0),
                reset_at: ms_to_secs(f64_of(detail, "resetTime")),
            });
        }
    }

    // 总体周限额：usage.{limit, remaining, resetTime}
    if let Some(usage) = json.get("usage").filter(|v| v.is_object()) {
        let limit = f64_of(usage, "limit").unwrap_or(1.0);
        let remaining = f64_of(usage, "remaining").unwrap_or(0.0);
        let used = (limit - remaining).max(0.0);
        let pct = if limit > 0.0 { used / limit * 100.0 } else { 0.0 };
        windows.push(WindowQuota {
            label: "7天".into(),
            used_percent: pct.clamp(0.0, 100.0),
            reset_at: ms_to_secs(f64_of(usage, "resetTime")),
        });
    }

    if windows.is_empty() {
        return Err("暂不支持：接口未返回窗口额度数据".into());
    }
    Ok(Quota::Windows { windows })
}
