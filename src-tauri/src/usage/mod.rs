//! 套餐查询统一入口：内置模板注册表 + 按模板分发查询。
//! 移植自 claude-mini-hud src/usage.ts。

pub mod alibaba;
pub mod balances;
pub mod claude_cache;
pub mod http;
pub mod kimi_coding;
pub mod minimax;
pub mod types;
pub mod xiaomi;
pub mod zhipu;

use types::{PlanConfig, Quota, Snapshot, Template};

/// 凭据（从系统凭据库加载）
#[derive(Debug, Default, Clone)]
pub struct Credential {
    pub bearer: Option<String>,
    pub cookie: Option<String>,
    pub ak_id: Option<String>,
    pub ak_secret: Option<String>,
}

/// 内置模板清单（≥5 个，当前 10 个）
pub fn templates() -> Vec<Template> {
    vec![
        Template {
            id: "minimax".into(),
            name: "MiniMax Coding Plan".into(),
            description: "5小时 / 周窗口剩余额度".into(),
            auth: "bearer".into(),
            quota_type: "windows".into(),
            has_region: true,
        },
        Template {
            id: "zhipu".into(),
            name: "智谱 GLM Coding Plan".into(),
            description: "5小时 / 周 / 月 / MCP 用量".into(),
            auth: "bearer".into(),
            quota_type: "windows".into(),
            has_region: true,
        },
        Template {
            id: "kimi-coding".into(),
            name: "Kimi For Coding".into(),
            description: "5小时窗口 + 周限额".into(),
            auth: "bearer".into(),
            quota_type: "windows".into(),
            has_region: false,
        },
        Template {
            id: "claude-cache".into(),
            name: "Claude (via claude-mini-hud)".into(),
            description: "读取 claude-mini-hud 本地缓存的 5h / 7天 限额，无需密钥".into(),
            auth: "none".into(),
            quota_type: "windows".into(),
            has_region: false,
        },
        Template {
            id: "xiaomi".into(),
            name: "小米 MiMo Token Plan".into(),
            description: "月度固定额度，推荐浏览器 Cookie 认证".into(),
            auth: "cookie".into(),
            quota_type: "fixed".into(),
            has_region: false,
        },
        Template {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: false,
        },
        Template {
            id: "kimi".into(),
            name: "Kimi / Moonshot".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: true,
        },
        Template {
            id: "stepfun".into(),
            name: "阶跃星辰 StepFun".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: true,
        },
        Template {
            id: "siliconflow".into(),
            name: "硅基流动 SiliconFlow".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: true,
        },
        Template {
            id: "alibaba".into(),
            name: "阿里云 DashScope".into(),
            description: "账户余额，需阿里云主账号 AK（BSS OpenAPI）".into(),
            auth: "bss".into(),
            quota_type: "balance".into(),
            has_region: false,
        },
    ]
}

/// 按套餐配置分发查询
pub async fn query(plan: &PlanConfig, cred: &Credential) -> Result<Quota, String> {
    let region = plan.region.as_str();
    match plan.template.as_str() {
        "minimax" => minimax::query(region, require(cred.bearer.as_deref(), "API Key")?).await,
        "zhipu" => {
            zhipu::query(
                region,
                plan.base_url.as_deref(),
                require(cred.bearer.as_deref(), "API Key")?,
            )
            .await
        }
        "kimi-coding" => {
            kimi_coding::query(require(cred.bearer.as_deref(), "API Key")?).await
        }
        "claude-cache" => claude_cache::query().await,
        "xiaomi" => xiaomi::query(cred.cookie.as_deref(), cred.bearer.as_deref()).await,
        "deepseek" => balances::deepseek(require(cred.bearer.as_deref(), "API Key")?).await,
        "kimi" => balances::kimi(region, require(cred.bearer.as_deref(), "API Key")?).await,
        "stepfun" => balances::stepfun(region, require(cred.bearer.as_deref(), "API Key")?).await,
        "siliconflow" => {
            balances::siliconflow(region, require(cred.bearer.as_deref(), "API Key")?).await
        }
        "alibaba" => {
            alibaba::query(
                require(cred.ak_id.as_deref(), "AccessKey ID")?,
                require(cred.ak_secret.as_deref(), "AccessKey Secret")?,
            )
            .await
        }
        other => Err(format!("未知模板: {other}")),
    }
}

fn require<'a>(v: Option<&'a str>, what: &str) -> Result<&'a str, String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("未配置{what}"))
}

/// 查询单个套餐，归一化为快照
pub async fn snapshot(plan: &PlanConfig, cred: &Credential) -> Snapshot {
    match query(plan, cred).await {
        Ok(q) => Snapshot::ok(&plan.id, q),
        Err(e) => Snapshot::fail(&plan.id, e),
    }
}
