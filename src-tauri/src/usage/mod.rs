//! 套餐查询统一入口：内置模板注册表 + 按模板分发查询。
//! 移植自 claude-mini-hud src/usage.ts，并对齐 cc-switch 的实现细节。

pub mod balances;
pub mod http;
pub mod kimi_coding;
pub mod minimax;
pub mod newapi;
pub mod official;
pub mod types;
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

/// 内置模板清单。窗口型"有啥显示啥"——接口不返回的窗口不展示。
pub fn templates() -> Vec<Template> {
    vec![
        Template {
            id: "minimax".into(),
            name: "MiniMax Coding Plan".into(),
            description: "5小时 / 周窗口剩余额度".into(),
            auth: "bearer".into(),
            quota_type: "windows".into(),
            has_region: true,
            needs_base_url: false,
            homepage: "https://platform.minimaxi.com".into(),
        },
        Template {
            id: "zhipu".into(),
            name: "智谱 GLM Coding Plan".into(),
            description: "5小时 / 周窗口用量（支持自定义 API 地址）".into(),
            auth: "bearer".into(),
            quota_type: "windows".into(),
            has_region: true,
            needs_base_url: true,
            homepage: "https://open.bigmodel.cn".into(),
        },
        Template {
            id: "kimi-coding".into(),
            name: "Kimi For Coding".into(),
            description: "5小时窗口 + 周限额".into(),
            auth: "bearer".into(),
            quota_type: "windows".into(),
            has_region: false,
            needs_base_url: false,
            homepage: "https://www.kimi.com/coding".into(),
        },
        Template {
            id: "claude-official".into(),
            name: "Claude Official".into(),
            description: "官方订阅 5小时 / 周限额（读取本机 Claude CLI 登录凭据）".into(),
            auth: "none".into(),
            quota_type: "windows".into(),
            has_region: false,
            needs_base_url: false,
            homepage: "https://claude.ai".into(),
        },
        Template {
            id: "codex".into(),
            name: "Codex / ChatGPT".into(),
            description: "官方订阅窗口额度（读取本机 Codex CLI 登录凭据）".into(),
            auth: "none".into(),
            quota_type: "windows".into(),
            has_region: false,
            needs_base_url: false,
            homepage: "https://chatgpt.com".into(),
        },
        Template {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: false,
            needs_base_url: false,
            homepage: "https://platform.deepseek.com".into(),
        },
        Template {
            id: "kimi".into(),
            name: "Kimi / Moonshot".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: true,
            needs_base_url: false,
            homepage: "https://platform.moonshot.cn".into(),
        },
        Template {
            id: "stepfun".into(),
            name: "阶跃星辰 StepFun".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: true,
            needs_base_url: false,
            homepage: "https://platform.stepfun.com".into(),
        },
        Template {
            id: "siliconflow".into(),
            name: "硅基流动 SiliconFlow".into(),
            description: "账户余额（按量计费）".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: true,
            needs_base_url: false,
            homepage: "https://cloud.siliconflow.cn".into(),
        },
        Template {
            id: "newapi".into(),
            name: "NewAPI / OneAPI 站点".into(),
            description: "填站点地址；币种跟随站点后台设置".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: false,
            needs_base_url: true,
            homepage: String::new(),
        },
        Template {
            id: "sub2api".into(),
            name: "Sub2API".into(),
            description: "需填写站点地址，走 OpenAI 兼容计费接口".into(),
            auth: "bearer".into(),
            quota_type: "balance".into(),
            has_region: false,
            needs_base_url: true,
            homepage: String::new(),
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
        "kimi-coding" => kimi_coding::query(require(cred.bearer.as_deref(), "API Key")?).await,
        "claude-official" => official::claude().await,
        "codex" => official::codex().await,
        "deepseek" => balances::deepseek(require(cred.bearer.as_deref(), "API Key")?).await,
        "kimi" => balances::kimi(region, require(cred.bearer.as_deref(), "API Key")?).await,
        "stepfun" => balances::stepfun(region, require(cred.bearer.as_deref(), "API Key")?).await,
        "siliconflow" => {
            balances::siliconflow(region, require(cred.bearer.as_deref(), "API Key")?).await
        }
        // NewAPI 系（OpenAI 兼容计费接口）
        "newapi" => {
            let base = plan
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("请在套餐配置中填写站点 API 地址")?;
            newapi::query(base, require(cred.bearer.as_deref(), "API Key")?).await
        }
        // Sub2API：CC Switch 同款 /v1/usage 提取器语义
        "sub2api" => {
            let base = plan
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("请在套餐配置中填写站点 API 地址")?;
            newapi::query_v1_usage(base, require(cred.bearer.as_deref(), "API Key")?).await
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
