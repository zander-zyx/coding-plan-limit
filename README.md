# Coding Plan Limit

一款跨平台（Windows / macOS / Linux）的本地桌面工具：**把你的多个 Coding Plan / Token Plan 套餐额度聚合到一个托盘悬停小窗中，一目了然。**

- 技术栈：Tauri 2（Rust 后端）+ 原生 HTML/CSS/JS（无前端框架，无构建步骤）
- 安装包约 5 MB，运行内存占用低（复用系统 WebView）
- 所有数据仅保存在本地；API 密钥存入**系统凭据库**（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）

## 下载安装（免打包）

打开 [Releases 页面](https://github.com/zander-zyx/coding-plan-limit/releases) 下载对应平台安装包：

| 平台 | 文件 |
|---|---|
| Windows | `*-x64-setup.exe`（NSIS 安装向导） |
| macOS (Apple Silicon) | `*aarch64.dmg` |
| macOS (Intel) | `*x64.dmg` |
| Linux | `*.AppImage` / `*.deb` |

每次推送 `v*` 标签会自动构建并发布全平台安装包。

## 功能一览

| 功能 | 说明 |
|---|---|
| 托盘常驻 | 关闭主窗口即驻留系统托盘；**Windows 悬停托盘图标弹出额度面板**（macOS / Linux 为左键点击——系统托盘 API 限制），每次打开自动刷新一轮 |
| 悬浮面板 | 固定展示 **1–10 家**自选套餐（点击卡片直达官网），其余收进"更多"折叠区 |
| 内置模板 | **15 个**内置模板（见下表），多数填入密钥即用 |
| 进度样式 | 平滑填充（默认，大号已用% + 通栏进度条）/ 环形百分比，设置里切换 |
| 统一语义 | 所有卡片统一显示**已使用 %**；窗口型套餐"有啥显示啥"，不支持的窗口不报错 |
| 自定义外观 | 主题（系统/浅色/深色）、主题色（预设 + 拾色器 + `#RRGGBB` 输入，弹窗实时跟随）、托盘图标 |
| 定时刷新 | 全局可配（默认 30 秒，最小 10 秒） |
| 阈值通知 | 每套餐可设阈值（默认剩余 10%）；提醒频率三选一：不通知 / 按时间间隔 / 按刷新次数 |

## 内置模板（15 个）

| 模板 | 数据内容 | 认证方式 |
|---|---|---|
| MiniMax Coding Plan | 5小时 / 周窗口 | API Key |
| 智谱 GLM Coding Plan | 5小时 / 周窗口（端点 `https://open.bigmodel.cn/api/monitor/usage/quota/limit`，支持自定义 API 地址） | API Key |
| Kimi For Coding | 5小时窗口 + 周限额 | API Key |
| **Claude Official** | 官方订阅 5小时 / 周限额（读取本机 `~/.claude/.credentials.json`，需 Claude CLI 已登录） | 无需密钥 |
| **Codex / ChatGPT** | 官方订阅窗口额度（读取本机 `~/.codex/auth.json`，需 Codex CLI 已登录） | 无需密钥 |
| Claude (via claude-mini-hud) | 5小时 / 7天限额（读取 [claude-mini-hud](https://github.com/zander-zyx/claude-mini-hud) 本地缓存） | 无需密钥 |
| 小米 MiMo Token Plan | 月度固定额度 | 浏览器 Cookie（推荐）或 API Key |
| DeepSeek | 账户余额 | API Key |
| Kimi / Moonshot | 账户余额 | API Key |
| 阶跃星辰 StepFun | 账户余额 | API Key |
| 硅基流动 SiliconFlow | 账户余额 | API Key |
| 阿里云 DashScope | 账户余额 | 阿里云主账号 AK（BSS OpenAPI） |
| **PackyCode** | 余额（OpenAI 兼容计费接口，默认 `https://www.packyapi.ai`） | API Key |
| **NewAPI / OneAPI 站点** | 余额（填任意 new-api 系站点地址） | API Key |
| **Sub2API** | 余额（填站点地址，需兼容 OpenAI 计费接口） | API Key |

> 查询逻辑移植自 [claude-mini-hud](https://github.com/zander-zyx/claude-mini-hud) 并对齐 [cc-switch](https://github.com/farion1231/cc-switch) 的实现细节（智谱端点、Claude/Codex 官方 OAuth usage 接口等）。

## 快速开始

### 1. 添加套餐

托盘菜单 →「打开主界面」→ 套餐管理 →「＋ 添加套餐」→ 选择模板 → 填入名称与密钥 → 保存。

- **Claude Official / Codex**：先在终端 `claude login` / `codex login`，应用自动读取本机登录凭据，无需填任何密钥
- **Cookie**（小米）：登录网页版 → F12 DevTools → 网络 → 复制请求头中的完整 `Cookie`
- **阿里云 AK**：需主账号 AccessKey，走 BSS OpenAPI，不是 DashScope Key
- **NewAPI 系**：填站点地址 + API Key；站点需兼容 `/v1/dashboard/billing/*` 接口

### 2. 悬浮窗展示选择

设置 →「悬浮弹窗」→ 勾选常驻套餐（最多 10 家）。其余在弹窗底部"更多（N）"展开；**点击弹窗卡片直接打开对应官网**。

### 3. 阈值与提醒

- 每个套餐的「提醒阈值」：窗口/固定额度型 = 剩余百分比下限（默认 10%）；余额型 = 余额下限
- 设置 →「提醒频率」：不通知 / 按间隔（默认 60 分钟）/ 按次数（默认每 10 次刷新）
- 窗口型告警取**剩余最少的窗口**判定；同一告警不刷屏，窗口重置后重新提醒

### 4. 外观

设置 →「外观」：主题、主题色（预设 / 拾色器 / `#RRGGBB`）、进度样式（平滑填充 / 环形）、托盘图标（选图即换）。悬浮窗实时跟随主题色变化。

## 数据与隐私

| 内容 | 位置 |
|---|---|
| 套餐配置 + 设置 | `%APPDATA%\com.zander.coding-plan-limit\config.json`（Win）/ `~/Library/Application Support/...`（mac）/ `~/.config/...`（Linux） |
| API 密钥 | 系统凭据库；凭据库不可用时降级明文存入 config.json（会弹出警告） |
| 最近快照 | 同目录 `snapshots.json`（用于冷启动即时显示） |

## 开发与构建

```bash
npm install
npm run dev      # 开发调试
npm run build    # 构建当前平台安装包
```

产物位置：`src-tauri/target/release/bundle/`。环境要求：Rust stable（+ MSVC / Xcode CLT / webkit2gtk）、Node.js ≥ 18。

## 已知限制

- macOS / Linux 托盘无系统级悬停事件，弹窗为左键点击触发（Windows 悬停正常）
- Claude Official / Codex 依赖本机 CLI 登录凭据（只读不刷新），token 过期后需重新 `claude login` / `codex login`
- NewAPI 系模板要求站点兼容 OpenAI 计费接口，不兼容的站点会给出明确报错
- 待支持：火山引擎 Coding Plan（AK/SK 签名）、AWS Bedrock（CloudWatch）、xAI Grok 订阅（OAuth/gRPC）
- 小米 Cookie 过期后需重新粘贴
