//! xAI Grok（SuperGrok）订阅额度：grok.com 私有 gRPC-web 计费端点。
//! 凭据来自本机 Grok CLI 登录文件（只读，不刷新 token，过期时提示重新登录）：
//!   ~/.grok/auth.json → 顶层 OIDC scope 键 → 条目 "key" 字段作 Bearer token。
//!
//! 响应为 gRPC-web protobuf（无官方 schema），对齐 cc-switch
//! services/subscription_grok.rs 的启发式解析：
//!   百分比 = wire 5(fixed32)、路径末段 field 1、值域 [0,100]，取最浅最早；
//!   重置时刻 = varint 且属未来 unix 秒，路径 [1,5,1] 优先，否则取最早；
//!   proto3 省略 0 值：有重置时刻 + 周期标记（[1,6,*] / [1,8,1] 值 1|2）读作 0%。
//! 窗口标签按重置剩余天数分桶：4–12 天 → 7天，20–45 天 → 本月，其余 → 总额度。

use super::http::post_bytes;
use super::types::{Quota, WindowQuota};

const GROK_BILLING_ENDPOINT: &str =
    "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";

// ─── 凭据：~/.grok/auth.json ──────────────────────────────────────────────

fn read_grok_token() -> Result<String, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "无法定位用户主目录".to_string())?;
    let path = std::path::Path::new(&home).join(".grok").join("auth.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| "未找到 Grok CLI 凭据（请先 grok login）".to_string())?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("凭据解析失败: {e}"))?;
    let Some(obj) = json.as_object() else {
        return Err("凭据结构异常：顶层不是对象".into());
    };
    // 顶层键是 OIDC scope 串；官方 auth.x.ai 前缀优先，其余含 sign-in 的次之
    let pick = |pred: &dyn Fn(&str) -> bool| {
        obj.iter()
            .filter(|(k, v)| {
                pred(k) && v.get("key").and_then(|x| x.as_str()).is_some_and(|s| !s.is_empty())
            })
            .map(|(_, v)| v["key"].as_str().unwrap_or_default().to_string())
            .next()
    };
    pick(&|k| k.starts_with("https://auth.x.ai::"))
        .or_else(|| pick(&|k| k.contains("/sign-in")))
        .or_else(|| pick(&|_| true))
        .ok_or_else(|| "凭据中无 access key（可能使用 API Key 登录）".to_string())
}

// ─── gRPC-web 帧 ──────────────────────────────────────────────────────────

/// 拆 gRPC-web 帧流 → (data 载荷合并, trailers 载荷列表)
fn split_grpc_frames(buf: &[u8]) -> (Vec<u8>, Vec<&[u8]>) {
    let mut data = Vec::new();
    let mut trailers = Vec::new();
    let mut pos = 0;
    while pos + 5 <= buf.len() {
        let flag = buf[pos];
        let len =
            u32::from_be_bytes([buf[pos + 1], buf[pos + 2], buf[pos + 3], buf[pos + 4]]) as usize;
        pos += 5;
        if pos + len > buf.len() {
            break;
        }
        let payload = &buf[pos..pos + len];
        pos += len;
        if flag & 0x80 != 0 {
            trailers.push(payload);
        } else {
            data.extend_from_slice(payload);
        }
    }
    (data, trailers)
}

/// trailers 帧（protobuf：field1=grpc-status varint, field2=message string）
fn grpc_status(trailers: &[&[u8]]) -> Option<(u64, String)> {
    for t in trailers {
        let mut pos = 0;
        let mut status = None;
        let mut msg = String::new();
        while pos < t.len() {
            let key = match read_varint(t, &mut pos) {
                Some(k) => k,
                None => break,
            };
            let number = (key >> 3) as u32;
            match (key & 7) as u8 {
                0 => match read_varint(t, &mut pos) {
                    Some(v) if number == 1 => status = Some(v),
                    Some(_) => {}
                    None => break,
                },
                2 => {
                    let len = match read_varint(t, &mut pos) {
                        Some(v) => v as usize,
                        None => break,
                    };
                    if pos + len > t.len() {
                        break;
                    }
                    if number == 2 {
                        msg = String::from_utf8_lossy(&t[pos..pos + len]).into_owned();
                    }
                    pos += len;
                }
                1 => pos += 8,
                5 => pos += 4,
                _ => break,
            }
        }
        if let Some(s) = status {
            return Some((s, msg));
        }
    }
    None
}

// ─── protobuf 启发式扫描 ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Field {
    number: u32,
    f32v: Option<f32>,
    varint: Option<u64>,
    /// wire 2 载荷；能完整按 message 解析时填充子字段，否则为字符串等叶子
    children: Option<Vec<Field>>,
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let b = *buf.get(*pos)?;
        *pos += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// 尝试把字节块按 protobuf message 线性解析；任何一步不合法则整体失败。
fn parse_fields(buf: &[u8], depth: u32) -> Option<Vec<Field>> {
    if depth > 8 {
        return None;
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let key = read_varint(buf, &mut pos)?;
        let number = (key >> 3) as u32;
        let wire = (key & 7) as u8;
        if number == 0 {
            return None;
        }
        match wire {
            0 => out.push(Field {
                number,
                f32v: None,
                varint: Some(read_varint(buf, &mut pos)?),
                children: None,
            }),
            1 => {
                pos += 8;
                if pos > buf.len() {
                    return None;
                }
            }
            2 => {
                let len = read_varint(buf, &mut pos)? as usize;
                let end = pos.checked_add(len)?;
                if end > buf.len() {
                    return None;
                }
                let sub = &buf[pos..end];
                pos = end;
                let children = parse_fields(sub, depth + 1);
                out.push(Field {
                    number,
                    f32v: None,
                    varint: None,
                    children,
                });
            }
            5 => {
                let end = pos.checked_add(4)?;
                if end > buf.len() {
                    return None;
                }
                let b: [u8; 4] = buf[pos..end].try_into().ok()?;
                pos = end;
                out.push(Field {
                    number,
                    f32v: Some(f32::from_le_bytes(b)),
                    varint: None,
                    children: None,
                });
            }
            _ => return None, // group（wire 3/4）与其余不合法
        }
    }
    Some(out)
}

#[derive(Debug, Default)]
struct Billing {
    percent: Option<f32>,
    /// 未来 unix 秒；exact = 路径命中 [1,5,1]
    reset: Option<i64>,
    reset_exact: bool,
    has_period_marker: bool,
}

/// 深度优先收集：百分比按"最浅路径、最早出现"→ 先扫本层直读字段再下钻。
fn collect(fields: &[Field], path: &mut Vec<u32>, now: i64, out: &mut Billing) {
    for f in fields {
        path.push(f.number);
        if f.number == 1 && out.percent.is_none() && f.f32v.is_some_and(|v| (0.0..=100.0).contains(&v))
        {
            out.percent = f.f32v;
        }
        if let Some(v) = f.varint {
            let ts = v as i64;
            let exact = path.ends_with(&[1, 5, 1]);
            if (1_700_000_000..=2_100_000_000).contains(&v) && ts > now {
                if exact && !out.reset_exact {
                    out.reset = Some(ts);
                    out.reset_exact = true;
                } else if !exact {
                    // 非精确路径取最早的未来时刻
                    out.reset = Some(out.reset.map_or(ts, |cur: i64| cur.min(ts)));
                }
            }
            // 周期标记：[1,6,*] 或 [1,8,1] 值 1|2
            let is_period = path.starts_with(&[1, 6]) || path.ends_with(&[1, 8, 1]);
            if is_period && (v == 1 || v == 2) {
                out.has_period_marker = true;
            }
        }
        path.pop();
    }
    for f in fields {
        if let Some(children) = &f.children {
            path.push(f.number);
            collect(children, path, now, out);
            path.pop();
        }
    }
}

/// 窗口标签按重置剩余天数分桶（与 cc-switch tier_name_for_reset 一致）
fn tier_label(reset_at: Option<i64>, now: i64) -> &'static str {
    if let Some(ts) = reset_at {
        let days = ((ts - now) as f64 / 86400.0).round() as i64;
        if (4..=12).contains(&days) {
            return "7天";
        }
        if (20..=45).contains(&days) {
            return "本月";
        }
    }
    "总额度"
}

pub async fn query() -> Result<Quota, String> {
    let token = read_grok_token()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let headers = [
        ("Authorization", format!("Bearer {token}")),
        ("Origin", "https://grok.com".into()),
        ("Referer", "https://grok.com/?_s=usage".into()),
        ("Content-Type", "application/grpc-web+proto".into()),
        ("x-grpc-web", "1".into()),
        ("x-user-agent", "connect-es/2.1.1".into()),
    ];
    // gRPC-web 空 message 请求 = 1 字节 flag + 4 字节大端长度 0
    let (status, body) = post_bytes(GROK_BILLING_ENDPOINT, &headers, vec![0u8; 5]).await?;
    if status == 401 || status == 403 {
        return Err(format!("Grok 凭据已过期，请重新 grok login（HTTP {status}）"));
    }
    if !(200..300).contains(&status) {
        return Err(format!("Grok 接口 HTTP {status}"));
    }

    let (data, trailers) = split_grpc_frames(&body);
    if let Some((code, msg)) = grpc_status(&trailers) {
        if code == 16 || (code == 7 && msg.to_lowercase().contains("cred")) {
            return Err(format!("Grok 凭据已过期，请重新 grok login（grpc {code}）"));
        }
    }

    let Some(fields) = parse_fields(&data, 0) else {
        return Err("暂不支持：未能解析 Grok 额度响应".into());
    };
    let mut found = Billing::default();
    collect(&fields, &mut Vec::new(), now, &mut found);
    // proto3 省略 0 值：有重置时刻 + 周期标记 → 读作 0%
    let percent =
        found.percent.unwrap_or_else(|| {
            if found.reset.is_some() && found.has_period_marker {
                0.0
            } else {
                -1.0
            }
        });
    if !(0.0..=100.0).contains(&percent) {
        return Err("暂不支持：接口未返回窗口额度数据".into());
    }

    Ok(Quota::Windows {
        windows: vec![WindowQuota {
            label: tier_label(found.reset, now).to_string(),
            used_percent: (percent.clamp(0.0, 100.0)) as f64,
            reset_at: found.reset,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        let mut v = v;
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        out
    }
    fn tag(field: u32, wire: u8) -> Vec<u8> {
        varint(((field << 3) | wire as u32) as u64)
    }
    fn len_delim(field: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = tag(field, 2);
        out.extend(varint(payload.len() as u64));
        out.extend_from_slice(payload);
        out
    }

    fn scan(payload: &[u8], now: i64) -> Billing {
        let fields = parse_fields(payload, 0).expect("parse");
        let mut found = Billing::default();
        collect(&fields, &mut Vec::new(), now, &mut found);
        found
    }

    /// root{ 1{ 1:fixed32=42.0, 5{1:varint=reset}, 6:varint=1 } }
    #[test]
    fn parses_percent_reset_and_period_marker() {
        let mut m1 = tag(1, 5);
        m1.extend(42.0f32.to_le_bytes());
        let mut inner5 = tag(1, 0);
        inner5.extend(varint(1_788_200_000));
        m1.extend(len_delim(5, &inner5));
        m1.extend(tag(6, 0));
        m1.extend(varint(1));
        let payload = len_delim(1, &m1);

        let now = 1_788_093_462;
        let found = scan(&payload, now);
        assert_eq!(found.percent, Some(42.0));
        assert_eq!(found.reset, Some(1_788_200_000));
        assert!(found.reset_exact);
        assert!(found.has_period_marker);
        // 剩余不足 1 天 → 非周/月窗口
        assert_eq!(tier_label(found.reset, now), "总额度");
    }

    /// 0% 特判：无 fixed32 百分比 + 有重置 + 有周期标记 → 0%
    #[test]
    fn zero_percent_via_period_marker() {
        let mut inner5 = tag(1, 0);
        inner5.extend(varint(1_788_093_462 + 5 * 86400));
        let mut inner8 = tag(1, 0);
        inner8.extend(varint(2)); // [1,8,1] = 2 → 周期标记
        let mut m1 = len_delim(5, &inner5);
        m1.extend(len_delim(8, &inner8));
        let payload = len_delim(1, &m1);

        let now = 1_788_093_462;
        let found = scan(&payload, now);
        assert!(found.percent.is_none());
        assert!(found.has_period_marker);
        let percent = found.percent.unwrap_or_else(|| {
            if found.reset.is_some() && found.has_period_marker {
                0.0
            } else {
                -1.0
            }
        });
        assert_eq!(percent, 0.0);
        // 剩余约 5 天 → 7天窗口
        assert_eq!(tier_label(found.reset, now), "7天");
    }

    /// 字符串字段不被误当嵌套 message（解析失败回退叶子）
    #[test]
    fn string_field_stays_leaf() {
        let payload = len_delim(2, b"hello world");
        let fields = parse_fields(&payload, 0).expect("parse");
        assert!(fields[0].children.is_none());
    }

    /// 非 [1,5,1] 路径的多个候选时间戳取最早
    #[test]
    fn non_exact_reset_takes_earliest() {
        let mut a = tag(2, 0);
        a.extend(varint(1_788_300_000));
        let mut b = tag(3, 0);
        b.extend(varint(1_788_250_000));
        let mut m1 = a;
        m1.extend(b);
        let payload = len_delim(1, &m1);

        let now = 1_788_093_462;
        let found = scan(&payload, now);
        assert_eq!(found.reset, Some(1_788_250_000));
        assert!(!found.reset_exact);
    }
}
