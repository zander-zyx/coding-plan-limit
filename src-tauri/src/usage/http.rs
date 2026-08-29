//! 零逻辑 HTTP 工具：共享 reqwest 客户端 + JSON GET。

use std::time::Duration;
use std::sync::OnceLock;

use reqwest::Client;

pub fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(8))
            .user_agent(concat!("coding-plan-limit/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client build")
    })
}

/// GET 并解析 JSON。headers 为 (名称, 值) 列表。
pub async fn get_json(
    url: &str,
    headers: &[(&str, String)],
) -> Result<serde_json::Value, String> {
    let mut req = client().get(url).header("Accept", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        let brief: String = text.chars().take(160).collect();
        return Err(format!("HTTP {}: {brief}", status.as_u16()));
    }
    serde_json::from_str(&text).map_err(|e| {
        let brief: String = text.chars().take(120).collect();
        format!("JSON 解析失败: {e}；响应内容: {brief}")
    })
}

/// 从 serde_json::Value 安全取 f64（数字或数字字符串）
pub fn f64_of(v: &serde_json::Value, key: &str) -> Option<f64> {
    match v.get(key) {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn str_of<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str())
}

/// 毫秒时间戳 → 秒
pub fn ms_to_secs(v: Option<f64>) -> Option<i64> {
    v.map(|ms| (ms / 1000.0) as i64).filter(|s| *s > 0)
}

/// 宽松时间解析：unix 秒 / 毫秒 / 数字字符串 / ISO8601 字符串 → 秒。
/// Kimi 的 resetTime 是 ISO 字符串，火山等返回秒或毫秒数字，统一在此收敛。
pub fn parse_time_any(v: Option<&serde_json::Value>) -> Option<i64> {
    let v = v?;
    let secs = |n: i64| {
        if n <= 0 {
            None
        } else if n < 1_000_000_000_000 {
            Some(n) // 秒
        } else {
            Some(n / 1000) // 毫秒
        }
    };
    if let Some(n) = v.as_i64() {
        return secs(n);
    }
    if let Some(n) = v.as_f64() {
        return secs(n as i64);
    }
    let s = v.as_str()?;
    if let Ok(ts) = s.trim().parse::<i64>() {
        return secs(ts);
    }
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}
