//! 阿里云 DashScope 账户余额：BSS OpenAPI QueryAccountBalance（RPC v1.0 签名）
//! 原逻辑参考 claude-mini-hud usage.ts queryAlibaba。

use base64::Engine;
use hmac::{Hmac, Mac};
use sha1::Sha1;

/// RFC3986 百分号编码
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn hmac_sha1_b64(key: &str, msg: &str) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(key.as_bytes()).expect("hmac key");
    mac.update(msg.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// UTC 时间 → "YYYY-MM-DDTHH:mm:ssZ"（time crate，无 chrono 依赖）
fn iso_now() -> String {
    let fmt = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day]T[hour]:[minute]:[second]Z",
    )
    .expect("format desc");
    time::OffsetDateTime::now_utc()
        .format(&fmt)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

pub async fn query(ak_id: &str, ak_secret: &str) -> Result<super::types::Quota, String> {
    use std::collections::BTreeMap;

    let nonce = format!(
        "{}{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        std::process::id()
    );

    // BTreeMap 保证按键排序（签名要求）
    let mut params: BTreeMap<&str, String> = BTreeMap::new();
    params.insert("Action", "QueryAccountBalance".into());
    params.insert("Format", "JSON".into());
    params.insert("Version", "2017-12-14".into());
    params.insert("AccessKeyId", ak_id.to_string());
    params.insert("SignatureMethod", "HMAC-SHA1".into());
    params.insert("SignatureVersion", "1.0".into());
    params.insert("SignatureNonce", nonce);
    params.insert("Timestamp", iso_now());

    let canonical = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let string_to_sign = format!("GET&{}&{}", percent_encode("/"), percent_encode(&canonical));
    let signature = hmac_sha1_b64(&format!("{ak_secret}&"), &string_to_sign);

    let url = format!(
        "https://business.aliyuncs.com/?{canonical}&Signature={}",
        percent_encode(&signature)
    );

    let json = super::http::get_json(&url, &[]).await?;
    if json.get("Success").and_then(|v| v.as_bool()) != Some(true) {
        let msg = json
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(format!("阿里云 BSS: {msg}"));
    }
    let data = json.get("Data").ok_or("响应缺少 Data")?;
    let amount: f64 = data
        .get("AvailableAmount")
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_f64()))
        .ok_or("缺少 AvailableAmount")?;
    let cash: Option<f64> = data
        .get("AvailableCashAmount")
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_f64()));
    let currency = data
        .get("Currency")
        .and_then(|v| v.as_str())
        .unwrap_or("CNY")
        .to_string();

    let note = cash.map(|c| format!("可用现金 ¥{c:.2}"));
    Ok(super::types::Quota::Balance {
        amount,
        currency,
        note,
    })
}
