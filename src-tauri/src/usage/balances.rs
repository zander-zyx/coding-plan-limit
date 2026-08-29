//! 余额类套餐查询：DeepSeek / Kimi(Moonshot) / StepFun / SiliconFlow
//! 原逻辑参考 claude-mini-hud usage.ts。

use super::http::{f64_of, get_json, str_of};
use super::types::Quota;

fn balance(amount: f64, currency: &str, note: Option<String>) -> Quota {
    Quota::Balance {
        amount,
        currency: currency.to_string(),
        note,
    }
}

/// DeepSeek 账户余额：balance_infos 按币种多条，优先 CNY 条目，币种透传
pub async fn deepseek(bearer: &str) -> Result<Quota, String> {
    let json = get_json(
        "https://api.deepseek.com/user/balance",
        &[("Authorization", format!("Bearer {bearer}"))],
    )
    .await?;
    let infos = json
        .get("balance_infos")
        .and_then(|v| v.as_array())
        .ok_or("响应缺少 balance_infos")?;
    let info = infos
        .iter()
        .find(|i| str_of(i, "currency") == Some("CNY"))
        .or_else(|| infos.first())
        .ok_or("balance_infos 为空")?;
    let total = f64_of(info, "total_balance").ok_or("缺少 total_balance")?;
    let currency = str_of(info, "currency").unwrap_or("CNY").to_string();
    let note = match (
        f64_of(info, "topped_up_balance"),
        f64_of(info, "granted_balance"),
    ) {
        (Some(t), Some(g)) => {
            Some(format!("充值 {}{t:.2} · 赠送 {}{g:.2}", symbol_of(&currency), currency))
        }
        _ => None,
    };
    Ok(balance(total, &currency, note))
}

fn symbol_of(currency: &str) -> &'static str {
    match currency {
        "CNY" => "¥",
        "USD" => "$",
        _ => "",
    }
}

/// Kimi / Moonshot 账户余额
pub async fn kimi(region: &str, bearer: &str) -> Result<Quota, String> {
    let host = if region == "intl" {
        "https://api.moonshot.ai"
    } else {
        "https://api.moonshot.cn"
    };
    let url = format!("{host}/v1/users/me/balance");
    let json = get_json(&url, &[("Authorization", format!("Bearer {bearer}"))]).await?;
    let d = json.get("data").unwrap_or(&json);
    let avail = f64_of(d, "available_balance")
        .or_else(|| f64_of(d, "balance"))
        .or_else(|| f64_of(d, "total_balance"))
        .ok_or("响应中无余额字段")?;
    let note = f64_of(d, "granted_balance").map(|g| format!("赠送 ¥{g:.2}"));
    Ok(balance(avail, if region == "intl" { "USD" } else { "CNY" }, note))
}

/// 阶跃星辰 StepFun 账户余额
pub async fn stepfun(region: &str, bearer: &str) -> Result<Quota, String> {
    let host = if region == "intl" {
        "https://api.stepfun.ai"
    } else {
        "https://api.stepfun.com"
    };
    let url = format!("{host}/v1/accounts");
    let json = get_json(&url, &[("Authorization", format!("Bearer {bearer}"))]).await?;
    let d = json.get("data").unwrap_or(&json);
    let bal = f64_of(d, "balance").ok_or("响应中无 balance 字段")?;
    let note = match (
        f64_of(d, "total_cash_balance"),
        f64_of(d, "total_voucher_balance"),
    ) {
        (Some(c), Some(v)) => Some(format!("现金 ¥{c:.2} · 代金券 ¥{v:.2}")),
        _ => None,
    };
    Ok(balance(bal, if region == "intl" { "USD" } else { "CNY" }, note))
}

/// 硅基流动 SiliconFlow 账户余额（totalBalance 为准，balance 字段有负值 bug）
pub async fn siliconflow(region: &str, bearer: &str) -> Result<Quota, String> {
    let host = if region == "intl" {
        "https://api.siliconflow.com"
    } else {
        "https://api.siliconflow.cn"
    };
    let url = format!("{host}/v1/user/info");
    let json = get_json(&url, &[("Authorization", format!("Bearer {bearer}"))]).await?;
    let d = json.get("data").unwrap_or(&json);
    let total = f64_of(d, "totalBalance").ok_or("响应中无 totalBalance 字段")?;
    let note = f64_of(d, "balance").map(|b| format!("充值 ¥{b:.2}"));
    let _ = str_of(d, "id"); // 保持字段引用完整性
    Ok(balance(total, if region == "intl" { "USD" } else { "CNY" }, note))
}
