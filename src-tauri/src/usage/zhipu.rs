//! 智谱 GLM Coding Plan：5h / 周 / 月 / MCP 用量
//! 解析对齐 cc-switch parse_zhipu_token_tiers：
//! unit 显式分类（3=5小时, 6=周）；unit 缺失进 unclassified 两段式兜底
//! （无 nextResetTime 优先补 5h 桶，其余按 reset 升序补空位）。

use super::http::{f64_of, get_json, ms_to_secs};
use super::types::{Quota, WindowQuota};

fn resolve_base(region: &str, base_url: Option<&str>) -> String {
    let raw = base_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if region == "intl" {
                "https://api.z.ai/api".to_string()
            } else {
                "https://open.bigmodel.cn/api".to_string()
            }
        });
    let trimmed = raw.trim_end_matches("/");
    let stripped = trimmed
        .strip_suffix("/anthropic")
        .map(|s| s.trim_end_matches("/"))
        .unwrap_or(trimmed);
    let lower = stripped.to_lowercase();
    if lower.contains("bigmodel.cn") {
        return "https://open.bigmodel.cn/api".into();
    }
    if lower.contains("z.ai") {
        return "https://api.z.ai/api".into();
    }
    stripped.to_string()
}

pub async fn query(region: &str, base_url: Option<&str>, bearer: &str) -> Result<Quota, String> {
    let url = format!("{}/monitor/usage/quota/limit", resolve_base(region, base_url));
    let json = get_json(
        &url,
        &[
            ("Authorization", format!("Bearer {bearer}")),
            ("Accept-Language", "en-US,en".into()),
        ],
    )
    .await?;

    if json.get("success").and_then(|v| v.as_bool()) != Some(true) {
        let msg = json
            .get("msg")
            .or_else(|| json.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(format!("智谱接口: {msg}"));
    }
    let data = json.get("data").ok_or("响应缺少 data")?;
    let limits = data
        .get("limits")
        .and_then(|v| v.as_array())
        .ok_or("响应缺少 data.limits")?;

    let is_quota = |t: &str| {
        t.eq_ignore_ascii_case("TOKENS_LIMIT") || t.eq_ignore_ascii_case("CREDIT_LIMIT")
    };

    let mut five_hour: Option<WindowQuota> = None;
    let mut weekly: Option<WindowQuota> = None;
    let mut monthly: Option<WindowQuota> = None;
    let mut mcp: Option<WindowQuota> = None;
    let mut unclassified: Vec<WindowQuota> = Vec::new();

    for item in limits {
        let typ = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let pct = f64_of(item, "percentage")
            .or_else(|| {
                let used = f64_of(item, "currentValue")?;
                let total = f64_of(item, "usage").filter(|t| *t > 0.0)?;
                Some(used / total * 100.0)
            });
        let reset = ms_to_secs(f64_of(item, "nextResetTime"));

        if typ.eq_ignore_ascii_case("TIME_LIMIT") {
            if let Some(p) = pct {
                mcp = Some(WindowQuota {
                    label: "MCP".into(),
                    used_percent: p.clamp(0.0, 100.0),
                    reset_at: reset,
                });
            }
            continue;
        }
        if !is_quota(typ) {
            continue;
        }
        let Some(p) = pct else { continue };
        let w = WindowQuota {
            label: String::new(),
            used_percent: p.clamp(0.0, 100.0),
            reset_at: reset,
        };
        match f64_of(item, "unit").map(|u| u as i64) {
            Some(3) if five_hour.is_none() => five_hour = Some(w),
            Some(6) if weekly.is_none() => weekly = Some(w),
            _ => unclassified.push(w),
        }
    }

    unclassified.sort_by_key(|w| (w.reset_at.is_some(), w.reset_at.unwrap_or(i64::MIN)));
    for w in unclassified {
        if five_hour.is_none() {
            five_hour = Some(w);
        } else if weekly.is_none() {
            weekly = Some(w);
        } else if monthly.is_none() {
            monthly = Some(w);
        }
    }

    let mut windows = Vec::new();
    if let Some(mut w) = five_hour {
        w.label = "5小时".into();
        windows.push(w);
    }
    if let Some(mut w) = weekly {
        w.label = "7天".into();
        windows.push(w);
    }
    if let Some(mut w) = monthly {
        w.label = "本月".into();
        windows.push(w);
    }
    if let Some(w) = mcp {
        windows.push(w);
    }

    if windows.is_empty() {
        return Err("暂不支持：接口未返回窗口额度数据".into());
    }
    Ok(Quota::Windows { windows })
}
