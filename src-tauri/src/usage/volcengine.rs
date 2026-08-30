//! 火山方舟 Agent Plan / Coding Plan 用量：控制面 OpenAPI（AK/SK 签名 V4）。
//! 协议对照 cc-switch services/coding_plan.rs（2026-06 实测）：
//!   · 网关 open.volcengineapi.com，POST 空 body，query 含 Action/Region/Version；
//!   · 签名 V4 火山变体——canonical headers 固定顺序 host;x-date;x-content-sha256;content-type
//!     （不按字母序）、算法串无 AWS4 前缀、kDate=HMAC(SK, date)、scope 结尾 request；
//!   · 自动判型：先 GetAFPUsage（Agent Plan，绝对值 Quota/Used），空/失败再
//!     GetCodingPlanUsage（Coding Plan，百分比 Level/Percent/ResetTimestamp）。

use hmac::{Hmac, Mac};
use reqwest::header::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::http::{client, f64_of, parse_time_any};
use super::types::{Quota, WindowQuota};

const OPENAPI_HOST: &str = "open.volcengineapi.com";
const API_VERSION: &str = "2024-01-01";
const REGION: &str = "cn-beijing";
const SERVICE: &str = "ark";
const CONTENT_TYPE: &str = "application/json; charset=utf-8";
const SIGNED_HEADERS: &str = "host;x-date;x-content-sha256;content-type";

type HmacSha256 = Hmac<Sha256>;

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// RFC3986 unreserved 之外全部按 %XX 编码（canonical query 用）
fn uri_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// canonical query 按 key 字母序（Action/Region/Version），同一份串既签名也拼 URL
fn canonical_query(action: &str) -> String {
    let mut pairs = [("Action", action), ("Region", REGION), ("Version", API_VERSION)];
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", uri_encode(k), uri_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// 火山签名 V4：返回直接可用的请求头（Authorization/X-Date/X-Content-Sha256/Content-Type）
fn sign_headers(ak: &str, sk: &str, query: &str, payload_hash: &str) -> HeaderMap {
    let now = time::OffsetDateTime::now_utc();
    let x_date = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let short_date = &x_date[..8];

    // 固定顺序 canonical headers（火山特有，不排序）
    let canonical_headers = format!(
        "host:{OPENAPI_HOST}\nx-date:{x_date}\nx-content-sha256:{payload_hash}\ncontent-type:{CONTENT_TYPE}\n"
    );
    let canonical_request =
        format!("POST\n/\n{query}\n{canonical_headers}\n{SIGNED_HEADERS}\n{payload_hash}");

    let scope = format!("{short_date}/{REGION}/{SERVICE}/request");
    let string_to_sign = format!(
        "HMAC-SHA256\n{x_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    // kDate = HMAC(SK, date)，SK 不加前缀；密钥派生终止串为 request
    let k_date = hmac_sha256(sk.as_bytes(), short_date.as_bytes());
    let k_region = hmac_sha256(&k_date, REGION.as_bytes());
    let k_service = hmac_sha256(&k_region, SERVICE.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"request");
    let signature = hex_lower(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "HMAC-SHA256 Credential={ak}/{scope}, SignedHeaders={SIGNED_HEADERS}, Signature={signature}"
    );

    let mut headers = HeaderMap::new();
    headers.insert("Authorization", authorization.parse().expect("header 值"));
    headers.insert("X-Date", x_date.parse().expect("header 值"));
    headers.insert(
        "X-Content-Sha256",
        payload_hash.parse().expect("header 值"),
    );
    headers.insert("Content-Type", CONTENT_TYPE.parse().expect("header 值"));
    headers
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 单次 OpenAPI 调用：2xx 且无 Error 信封返回 Ok(Result)；其余给出含错误码的信息
async fn openapi_call(action: &str, ak: &str, sk: &str) -> Result<Value, String> {
    let query = canonical_query(action);
    let url = format!("https://{OPENAPI_HOST}/?{query}");
    let payload_hash = sha256_hex(b"");
    let headers = sign_headers(ak, sk, &query, &payload_hash);

    let resp = client()
        .post(&url)
        .headers(headers)
        .body(b"" as &[u8])
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    let body: Value = serde_json::from_str(&text)
        .map_err(|_| format!("HTTP {}: 响应非 JSON: {}", status.as_u16(), brief(&text)))?;

    // 火山网关对签名/凭据错误常返 4xx（多为 400）而非 401/403，且 200 路径也
    // 可能带业务错误信封，统一从 ResponseMetadata.Error 提取
    let envelope_err = body
        .pointer("/ResponseMetadata/Error")
        .or_else(|| body.get("Error"))
        .filter(|e| e.get("Code").is_some() || e.get("Message").is_some());
    if let Some(err) = envelope_err {
        let code = err.get("Code").and_then(|v| v.as_str()).unwrap_or("");
        let msg = err.get("Message").and_then(|v| v.as_str()).unwrap_or("");
        return Err(format!("{action} 失败 ({code}): {msg}"));
    }
    if !status.is_success() {
        return Err(format!("{action} 失败: HTTP {}: {}", status.as_u16(), brief(&text)));
    }
    Ok(body.get("Result").unwrap_or(&body).clone())
}

fn brief(text: &str) -> String {
    let s: String = text.chars().take(160).collect();
    s
}

/// GetAFPUsage（Agent Plan）：AFPFiveHour/AFPWeekly/AFPDate(跳过)/AFPMonthly，
/// Quota/Used 为绝对值；Quota<=0 视为未订阅该窗口
fn parse_afp_windows(result: &Value) -> Vec<WindowQuota> {
    let mut windows = Vec::new();
    for (key, label) in [
        ("AFPFiveHour", "5小时"),
        ("AFPWeekly", "7天"),
        ("AFPMonthly", "30天"),
    ] {
        let Some(win) = result.get(key) else { continue };
        let quota = f64_of(win, "Quota").unwrap_or(0.0);
        if quota <= 0.0 {
            continue;
        }
        let used = f64_of(win, "Used").unwrap_or(0.0);
        windows.push(WindowQuota {
            label: label.into(),
            used_percent: used / quota * 100.0,
            reset_at: parse_time_any(win.get("ResetTime")),
        });
    }
    windows
}

/// Level 窗口标签归一（实测 2026-06：session/weekly/monthly）
fn level_label(level: &str) -> Option<&'static str> {
    match level.to_lowercase().as_str() {
        "session" | "5h" | "fivehour" | "five_hour" | "rolling_5h" => Some("5小时"),
        "weekly" | "week" | "7d" => Some("7天"),
        "monthly" | "month" => Some("30天"),
        _ => None,
    }
}

/// GetCodingPlanUsage（Coding Plan）：官方无逐字段规格，防御式宽松匹配
fn parse_coding_windows(result: &Value) -> Vec<WindowQuota> {
    let Some(arr) = result
        .get("QuotaUsage")
        .and_then(|v| v.as_array())
        .or_else(|| result.get("Usages").and_then(|v| v.as_array()))
        .or_else(|| result.get("Details").and_then(|v| v.as_array()))
    else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    for item in arr {
        let level = item
            .get("Level")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("Type").and_then(|v| v.as_str()))
            .or_else(|| item.get("Period").and_then(|v| v.as_str()))
            .unwrap_or("");
        let Some(label) = level_label(level) else { continue };
        let percent = f64_of(item, "Percent")
            .or_else(|| f64_of(item, "UsedPercent"))
            .or_else(|| f64_of(item, "UsagePercent"))
            .unwrap_or(0.0);
        windows.push(WindowQuota {
            label: label.into(),
            used_percent: percent,
            reset_at: item
                .get("ResetTime")
                .or_else(|| item.get("ResetTimestamp"))
                .and_then(|v| parse_time_any(Some(v))),
        });
    }
    windows
}

/// 有窗口数据则重排为 5小时 → 7天 → 30天（其余排后），无数据原样返回
fn sort_windows(windows: &mut [WindowQuota]) {
    let order = |label: &str| match label {
        "5小时" => 0,
        "7天" => 1,
        "30天" => 2,
        _ => 3,
    };
    windows.sort_by_key(|w| order(&w.label));
}

pub async fn query(ak: &str, sk: &str) -> Result<Quota, String> {
    // 自动判型：Agent Plan 探测（空结果不算失败），失败记录后回退 Coding Plan
    let mut first_err: Option<String> = None;
    match openapi_call("GetAFPUsage", ak, sk).await {
        Ok(result) => {
            let mut windows = parse_afp_windows(&result);
            if !windows.is_empty() {
                sort_windows(&mut windows);
                return Ok(Quota::Windows { windows });
            }
        }
        Err(e) => first_err = Some(e),
    }

    let result = openapi_call("GetCodingPlanUsage", ak, sk).await?;
    let mut windows = parse_coding_windows(&result);
    if windows.is_empty() {
        return Err(match first_err {
            Some(e) => e,
            None => format!(
                "接口未返回窗口额度数据（未订阅 Agent/Coding Plan？）: {}",
                brief(&result.to_string())
            ),
        });
    }
    sort_windows(&mut windows);
    Ok(Quota::Windows { windows })
}
