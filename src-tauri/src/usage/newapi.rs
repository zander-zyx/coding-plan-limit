//! NewAPI 系站点余额查询（OpenAI 兼容计费接口）。
//! 适用于 new-api / one-api 及其衍生面板（PackyCode、Sub2API 等兼容站点）：
//!   GET {base}/v1/dashboard/billing/subscription → hard_limit_usd（总额度，USD）
//!   GET {base}/v1/dashboard/billing/usage        → total_usage（已用，单位: 美分）

use super::http::{f64_of, get_json};
use super::types::Quota;

pub async fn query(base_url: &str, bearer: &str) -> Result<Quota, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("需要在套餐中填写站点 API 地址".into());
    }

    let auth = [("Authorization", format!("Bearer {bearer}"))];
    let sub = get_json(&format!("{base}/v1/dashboard/billing/subscription"), &auth).await?;
    let total = f64_of(&sub, "hard_limit_usd")
        .or_else(|| f64_of(&sub, "total_granted"))
        .ok_or("响应缺少 hard_limit_usd（站点可能不兼容 OpenAI 计费接口）")?;

    let usage = get_json(&format!("{base}/v1/dashboard/billing/usage"), &auth).await?;
    let used_cents = f64_of(&usage, "total_usage").unwrap_or(0.0);
    let used = used_cents / 100.0;

    let remaining = (total - used).max(0.0);
    Ok(Quota::Balance {
        amount: remaining,
        currency: "USD".into(),
        note: Some(format!("已用 ${used:.2} / 共 ${total:.2}")),
    })
}
