//! Kimi For Coding 用量：5小时窗口 + 周限额
//! 原逻辑参考 claude-mini-hud usage.ts queryKimiCoding。

use super::http::{f64_of, get_json, ms_to_secs};
use super::types::{Quota, WindowQuota};

pub async fn query(bearer: &str) -> Result<Quota, String> {
    let url = "https://api.kimi.com/coding/v1/usages";
    let json = get_json(url, &[("Authorization", format!("Bearer {bearer}"))]).await?;

    let mut windows: Vec<WindowQuota> = Vec::new();

    // 5 小时窗口：limits[] 中带 detail 的项（与原逻辑一致，取最后一个）
    if let Some(limits) = json.get("limits").and_then(|v| v.as_array()) {
        for item in limits {
            let Some(detail) = item.get("detail") else {
                continue;
            };
            let limit = f64_of(detail, "limit").unwrap_or(1.0);
            let remaining = f64_of(detail, "remaining").unwrap_or(0.0);
            let used = (limit - remaining).max(0.0);
            let pct = if limit > 0.0 { used / limit * 100.0 } else { 0.0 };
            windows.push(WindowQuota {
                label: "5小时".into(),
                used_percent: pct.clamp(0.0, 100.0),
                reset_at: reset_time_of(detail, "resetTime"),
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
            reset_at: reset_time_of(usage, "resetTime"),
        });
    }

    if windows.is_empty() {
        return Err("暂不支持：接口未返回窗口额度数据".into());
    }
    Ok(Quota::Windows { windows })
}

/// resetTime 兼容解析：接口实际返回 ISO 8601 字符串（如 2026-02-11T17:32:50.757941Z），
/// 旧字段文档写的是毫秒数字——两者都支持
fn reset_time_of(v: &serde_json::Value, key: &str) -> Option<i64> {
    match v.get(key) {
        Some(serde_json::Value::Number(n)) => ms_to_secs(n.as_f64()),
        Some(serde_json::Value::String(s)) => {
            let s = s.trim();
            if let Ok(ms) = s.parse::<f64>() {
                return ms_to_secs(Some(ms));
            }
            use time::format_description::well_known::Rfc3339;
            time::OffsetDateTime::parse(s, &Rfc3339)
                .ok()
                .map(|dt| dt.unix_timestamp())
                .filter(|t| *t > 0)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso_reset_time() {
        let json = serde_json::json!({ "resetTime": "2026-02-11T17:32:50.757941Z" });
        let t = reset_time_of(&json, "resetTime").unwrap();
        assert!(t > 1_770_000_000);
    }

    #[test]
    fn parse_millis_reset_time() {
        let json = serde_json::json!({ "resetTime": 1_770_000_000_000_f64 });
        assert_eq!(reset_time_of(&json, "resetTime"), Some(1_770_000_000));
    }

    #[test]
    fn missing_or_bad_reset_time_is_none() {
        assert_eq!(reset_time_of(&serde_json::json!({}), "resetTime"), None);
        assert_eq!(
            reset_time_of(&serde_json::json!({ "resetTime": "not-a-time" }), "resetTime"),
            None
        );
    }
}
