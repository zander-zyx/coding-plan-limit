# Plan Limit

一款跨平台（Windows / macOS / Linux）的本地桌面工具：**把你的多个 AI Coding Plan / Token Plan 套餐额度聚合到一个托盘悬停小窗中，一目了然。**

- 技术栈：Tauri 2（Rust 后端）+ 原生 HTML/CSS/JS（无前端框架，无构建步骤）
- 安装包约 5 MB，运行内存占用低（复用系统 WebView）
- 所有数据仅保存在本地；API 密钥存入**系统凭据库**（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）
- 设计语言：Kimi Twilight（深空紫 × 原生质感）

## 下载安装（免打包）

打开 [Releases 页面](https://github.com/zander-zyx/coding-plan-limit/releases) 下载对应平台安装包：

| 平台 | 文件 |
|---|---|
| Windows | `*-x64-setup.exe`（NSIS 向导，**可自行修改安装目录**） |
| macOS (Apple Silicon) | `*aarch64.dmg`（拖入 Applications） |
| macOS (Intel) | `*x64.dmg` |
| Linux | `*.AppImage` / `*.deb` |

每次推送 `v*` 标签会自动构建并发布全平台安装包。

### 升级

内置更新：启动时 + 每 24 小时后台静默检查（可关）。有新版本时**主窗口侧边 / 弹窗标题栏出现更新按钮**，点击 = **应用内直接下载当前平台安装包**（按钮实时显示百分比），Windows 下载完成自动启动安装向导；也可在 托盘菜单 / 关于页 手动检查。

## 功能一览

| 功能 | 说明 |
|---|---|
| 托盘常驻 | 关闭主窗口即驻留托盘；**Windows 悬停图标弹出面板**（macOS/Linux 左键点击——系统 API 限制），打开即节流刷新 |
| 悬浮面板 | 固定展示 **1–10 家**自选套餐（未选默认 3 家，顺序跟随列表拖拽排序），点击卡片直达官网，其余收进"更多"折叠区 |
| 窗口明细 | **5小时 → 7天 → 月 → MCP** 固定排序，每窗口独立行（标签+进度条+百分比+重置时刻）；"有啥显示啥"，不支持的不报错 |
| 内置模板 | **15 个**（见下表），填密钥即用 |
| 拖拽排序 | 套餐列表拖拽排序，持久化并决定弹窗展示/补足顺序 |
| 进度样式 | 平滑填充（默认）/ 环形（主窗口），弹窗统一行式 |
| 阈值通知 | 每套餐阈值（窗口型=剩余%、余额型=金额下限）；提醒频率三选一：不通知 / 按间隔 / 按次数；同告警不刷屏，紧急行变暖琥珀色 |
| 自定义外观 | 主题（系统/浅色/深色）、主题色（预设+拾色器+#RRGGBB，**弹窗实时跟随**）、Logo 四态（原色/单色/Mark/自定义图片，托盘+标题栏+侧边同步） |
| 数据本地 | 配置 `config.json`（损坏自动备份并拒绝写入）、密钥系统凭据库、快照 `snapshots.json` |

## 内置模板（15 个）

| 模板 | 数据内容 | 认证方式 |
|---|---|---|
| MiniMax Coding Plan | 5小时 / 7天窗口（周窗口未激活不显示） | API Key |
| 智谱 GLM Coding Plan | 5小时 / 7天 / 本月 / MCP（unit 缺失自动兜底分类） | API Key |
| Kimi For Coding | 5小时窗口 + 7天限额（重置时刻） | API Key |
| **Claude Official** | 官方订阅 5小时 / 7天 / Opus / Sonnet（读本机 `~/.claude/.credentials.json`，需 Claude CLI 已登录） | 无需密钥 |
| **Codex / ChatGPT** | 官方订阅窗口额度（读本机 `~/.codex/auth.json`，需 Codex CLI 已登录） | 无需密钥 |
| Claude (via claude-mini-hud) | 5小时 / 7天限额（读取 [claude-mini-hud](https://github.com/zander-zyx/claude-mini-hud) 本地缓存） | 无需密钥 |
| 小米 MiMo Token Plan | 月度固定额度 | 浏览器 Cookie（推荐）或 API Key |
| DeepSeek | 账户余额（多币种优先 CNY） | API Key |
| Kimi / Moonshot | 账户余额 | API Key |
| 阶跃星辰 StepFun | 账户余额 | API Key |
| 硅基流动 SiliconFlow | 账户余额（国际站 USD） | API Key |
| 阿里云 DashScope | 账户余额 | 阿里云主账号 AK（BSS OpenAPI） |
| **PackyCode** | 余额（OpenAI 兼容计费接口，默认 `https://www.packyapi.ai`） | API Key |
| **NewAPI / OneAPI 站点** | 余额（填站点地址；币种跟随站点后台显示设置） | API Key |
| **Sub2API** | 余额（填站点地址，需兼容 OpenAI 计费接口） | API Key |

> 查询逻辑移植自 [claude-mini-hud](https://github.com/zander-zyx/claude-mini-hud)，并逐字段对齐 [cc-switch](https://github.com/farion1231/cc-switch) 的实现（智谱 unit 分类、MiniMax 周窗口门控、Claude/Codex OAuth usage 接口等）。

## 快速开始

1. **添加套餐**：托盘菜单 →「打开主界面」→ 套餐 →「＋ 添加套餐」→ 选品牌 → 填名称与密钥 → 保存
   - Claude Official / Codex：先在终端 `claude login` / `codex login`，应用自动读本机凭据
   - 小米：登录网页版 → F12 → 复制请求头完整 `Cookie`
   - 阿里云：主账号 AccessKey（BSS OpenAPI），不是 DashScope Key
   - NewAPI 系：填站点地址 + API Key（站点需兼容 `/v1/dashboard/billing/*`）
2. **悬浮窗**：设置 → 悬浮弹窗 → 勾选常驻套餐（1–10 家）；拖拽套餐列表调顺序，弹窗同步；点卡片跳官网
3. **通知**：每套餐设阈值（默认剩余 10%）；提醒频率按需选择；紧急窗口/进度条自动变暖琥珀色
4. **外观**：主题 / 主题色（含 `#RRGGBB` 输入）/ 进度样式 / Logo 四态（原色、单色白、Mark、自定义图片）

## 数据与隐私

| 内容 | 位置 |
|---|---|
| 套餐配置 + 设置 | `%APPDATA%\com.zander.coding-plan-limit\config.json`（Win）/ `~/Library/Application Support/...`（mac）/ `~/.config/...`（Linux） |
| API 密钥 | 系统凭据库；凭据库不可用时降级明文存入 config.json（会弹警告），凭据库恢复后自动清除明文 |
| 最近快照 | 同目录 `snapshots.json` |
| 配置损坏保护 | config.json 损坏时自动备份为 `config.corrupt.bak` 并**拒绝写入**，绝不静默丢数据 |

## 开发与构建

```bash
npm install
npm run dev      # 开发调试
npm run build    # 构建当前平台安装包
```

环境要求：Rust stable（+ MSVC / Xcode CLT / webkit2gtk）、Node.js ≥ 18。

辅助脚本：`scripts/render-mark.mjs`（Logo Mark 光栅化）、`scripts/fetch-logos*.mjs`（品牌图标抓取）、`scripts/make-icon.ps1`（默认图标）、`dev-preview/index.html`（浏览器设计预览）。

## 已知限制

- macOS / Linux 托盘无系统悬停事件，弹窗为左键点击触发（Windows 悬停正常）
- macOS 的 Claude 凭据在钥匙串，claude-official 模板目前仅读取文件凭据（Windows/Linux）
- NewAPI 系币种跟随站点后台显示设置（API 无法自省）
- 待支持：火山引擎 Coding Plan（AK/SK 签名）、AWS Bedrock（CloudWatch）、xAI Grok 订阅（OAuth/gRPC）
- 小米 Cookie 过期需重新粘贴；Claude/Codex 凭据过期会提示重新 login
