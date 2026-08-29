//! 数据模型：统一额度表示、快照、套餐配置、内置模板元数据。

use serde::{Deserialize, Serialize};

/// 一个统计窗口（如 5 小时 / 本周 / 本月）的已用百分比
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowQuota {
    pub label: String,
    /// 已用百分比 0-100
    pub used_percent: f64,
    /// 窗口重置时间（unix 秒）
    pub reset_at: Option<i64>,
}

/// 统一额度表示：窗口型 / 余额型 / 固定额度型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Quota {
    /// 多窗口百分比（Coding Plan 类：5h / 周 / 月）
    Windows { windows: Vec<WindowQuota> },
    /// 账户余额（按量计费类）
    Balance {
        amount: f64,
        currency: String,
        note: Option<String>,
    },
    /// 固定总额度（Token Plan 类）
    FixedQuota {
        used_percent: f64,
        used: f64,
        total: f64,
        unit: String,
        reset_at: Option<i64>,
    },
}

/// 一次查询的结果快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub plan_id: String,
    pub ok: bool,
    pub error: Option<String>,
    pub quota: Option<Quota>,
    pub updated_at: i64,
}

impl Snapshot {
    pub fn ok(plan_id: &str, quota: Quota) -> Self {
        Snapshot {
            plan_id: plan_id.to_string(),
            ok: true,
            error: None,
            quota: Some(quota),
            updated_at: now_secs(),
        }
    }

    pub fn fail(plan_id: &str, error: String) -> Self {
        Snapshot {
            plan_id: plan_id.to_string(),
            ok: false,
            error: Some(error),
            quota: None,
            updated_at: now_secs(),
        }
    }
}

/// 内置模板元数据（供前端渲染"添加套餐"表单）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    /// bearer | cookie | bss | none
    pub auth: String,
    /// windows | balance | fixed
    pub quota_type: String,
    /// 是否支持国内/国际站切换
    pub has_region: bool,
    /// 是否需要填写 API 地址（zhipu 可选；newapi 系必填）
    pub needs_base_url: bool,
    /// 官网首页（弹窗点击卡片跳转）
    #[serde(default)]
    pub homepage: String,
}

/// 用户套餐配置（密钥不在此存，走系统凭据库）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanConfig {
    #[serde(default)]
    pub id: String,
    pub template: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 提醒阈值：窗口/固定额度型为已用百分比；余额型为货币金额下限
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    /// cn | intl
    #[serde(default = "default_region")]
    pub region: String,
    /// 可选：覆盖 API 基础地址（如自建代理 / 智谱自定义 base）
    #[serde(default)]
    pub base_url: Option<String>,
    /// 自定义套餐 logo（PNG dataURL，由前端压缩后传入；None = 使用内置品牌图标）
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

fn default_true() -> bool {
    true
}
/// 提醒阈值默认 10%：窗口/固定额度型为"剩余百分比下限"，余额型为"余额金额下限"
fn default_threshold() -> f64 {
    10.0
}
fn default_region() -> String {
    "cn".to_string()
}

/// 发给前端的视图：套餐配置 + 最新快照
#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub plan: PlanConfig,
    pub snapshot: Option<Snapshot>,
}

/// 通知频率模式：不通知 / 按时间间隔 / 按刷新次数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyMode {
    Off,
    Interval,
    Count,
}

/// 全局设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 自动刷新间隔（秒），默认 30，最小 10
    #[serde(default = "default_refresh_seconds")]
    pub refresh_seconds: u32,
    #[serde(default)]
    pub notify_mode: NotifyMode,
    /// Interval 模式：同一告警的重复提醒间隔（分钟）
    #[serde(default = "default_notify_interval")]
    pub notify_interval_minutes: u32,
    /// Count 模式：同一告警每 N 次刷新提醒一次
    #[serde(default = "default_notify_count")]
    pub notify_count: u32,
    #[serde(default)]
    pub autostart: bool,
    /// 主题：system | light | dark
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 悬浮弹窗固定展示的套餐 id（1-10 家，按顺序展示；其余收进"更多"）
    #[serde(default)]
    pub popup_plan_ids: Vec<String>,
    /// 自定义主题色（#RRGGBB；None = 默认蓝）
    #[serde(default)]
    pub accent: Option<String>,
    /// 卡片进度样式：bar（平滑填充，默认）| ring（环形百分比）
    #[serde(default = "default_bar_style")]
    pub bar_style: String,
    /// 自动检查更新（启动时 + 每 24 小时，有新版本才提醒）
    #[serde(default = "default_true")]
    pub auto_check_update: bool,
    /// 自定义托盘图标（PNG dataURL；None = 使用内置默认图标）
    #[serde(default)]
    pub custom_icon: Option<String>,
    /// Logo 样式：color（原色，默认）| mark（Z 标）| custom（应用 custom_icon 图片）；custom_icon 与样式选择独立存档
    #[serde(default = "default_logo_style")]
    pub logo_style: String,
}

fn default_logo_style() -> String {
    "color".to_string()
}

fn default_theme() -> String {
    "system".to_string()
}
fn default_bar_style() -> String {
    "bar".to_string()
}

fn default_refresh_seconds() -> u32 {
    30
}
fn default_notify_interval() -> u32 {
    60
}
fn default_notify_count() -> u32 {
    10
}
impl Default for NotifyMode {
    fn default() -> Self {
        NotifyMode::Interval
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            refresh_seconds: default_refresh_seconds(),
            notify_mode: NotifyMode::Interval,
            notify_interval_minutes: default_notify_interval(),
            notify_count: default_notify_count(),
            autostart: false,
            theme: default_theme(),
            custom_icon: None,
            popup_plan_ids: Vec::new(),
            accent: None,
            bar_style: default_bar_style(),
            logo_style: default_logo_style(),
            auto_check_update: true,
        }
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
