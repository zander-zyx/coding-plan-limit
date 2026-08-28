# Coding Plan Limit

一款跨平台（Windows / macOS / Linux）的本地桌面工具：**把你的多个 Coding Plan / Token Plan 套餐额度聚合到一个托盘悬停小窗中，一目了然。**

- 技术栈：Tauri 2（Rust 后端）+ 原生 HTML/CSS/JS（无前端框架，无构建步骤）
- 安装包约 5 MB，运行内存占用低（复用系统 WebView）
- 所有数据仅保存在本地，不上传任何信息；API 密钥存入**系统凭据库**（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）

![平台](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)

---

## 功能一览

| 功能 | 说明 |
|---|---|
| 托盘常驻 | 关闭主窗口即驻留系统托盘；**Windows 悬停托盘图标弹出额度面板**（macOS / Linux 为左键点击弹出——系统托盘 API 限制） |
| 悬浮面板 | 固定展示 ≤2 家自选套餐（大卡片），其余收进"更多"折叠区 |
| 内置模板 | 预置 10 个主流平台模板（见下表），填入密钥即用 |
| 平滑进度条 | 大号百分比 + 通栏进度条 + 重置倒计时，参考各控制台风格 |
| 自定义外观 | 主题（系统/浅色/深色）、主题色（任意色）、托盘图标（自选图片） |
| 定时刷新 | 全局可配（默认 30 秒，最小 10 秒） |
| 阈值通知 | 每套餐可设阈值；提醒频率三选一：不通知 / 按时间间隔 / 按刷新次数 |
| 数据本地 | 配置存 `config.json`，密钥存系统凭据库，快照缓存 `snapshots.json` |

## 内置模板（10 个）

| 模板 | 数据内容 | 认证方式 |
|---|---|---|
| MiniMax Coding Plan | 5小时 / 周 / 月窗口剩余 | API Key |
| 智谱 GLM Coding Plan | 5小时 / 周 / 月 / MCP 用量 | API Key（支持自定义 API 地址） |
| Kimi For Coding | 5小时窗口 + 周限额 | API Key |
| Claude（经 claude-mini-hud） | 5小时 / 7天限额 | 无需密钥（读取 [claude-mini-hud](https://github.com/zander-zyx/claude-mini-hud) 本地缓存） |
| 小米 MiMo Token Plan | 月度固定额度 | 浏览器 Cookie（推荐）或 API Key |
| DeepSeek | 账户余额 | API Key |
| Kimi / Moonshot | 账户余额 | API Key（国内/国际站） |
| 阶跃星辰 StepFun | 账户余额 | API Key |
| 硅基流动 SiliconFlow | 账户余额 | API Key |
| 阿里云 DashScope | 账户余额 | 阿里云主账号 AK（BSS OpenAPI） |

> 查询逻辑移植自 [claude-mini-hud](https://github.com/zander-zyx/claude-mini-hud)（同为本人项目），各平台 endpoint / 鉴权 / 解析与其保持一致。
> Claude 官方不提供独立额度 API，故采用读取 claude-mini-hud 缓存的兼容方案；两个项目同时使用时数据自动互通。

## 快速开始

### 1. 添加套餐

托盘菜单 →「打开主界面」→ 套餐管理 →「＋ 添加套餐」→ 选择模板 → 填入名称与密钥 → 保存。

- **API Key**：在各平台控制台创建（DeepSeek / 智谱 / Moonshot 等均支持查询用量/余额的普通 API Key）
- **Cookie**（小米）：登录网页版 → F12 DevTools → 网络 → 复制请求头中的完整 `Cookie`
- **阿里云 AK**：需主账号 AccessKey（RAM 密钥），走 BSS OpenAPI 查账户余额，不是 DashScope Key

### 2. 悬浮窗选择展示哪两家

设置 →「悬浮弹窗」→ 勾选 2 家固定展示。其余套餐在弹窗底部"更多（N）"展开查看。

### 3. 阈值与提醒

- 每个套餐的「提醒阈值」：窗口/固定额度型 = 剩余百分比下限（默认 10%）；余额型 = 余额下限
- 设置 →「提醒频率」：不通知 / 按间隔（默认 60 分钟）/ 按次数（默认每 10 次刷新）
- 同一告警不会刷屏；窗口重置或告警解除后再次触发会重新提醒

### 4. 外观

设置 →「外观」：主题（系统/浅色/深色）、主题色（预设 + 任意自定义色）、托盘图标（选择本地图片即换）。

## 数据与隐私

| 内容 | 位置 |
|---|---|
| 套餐配置 + 设置 | `%APPDATA%\com.zander.coding-plan-limit\config.json`（Win）/ `~/Library/Application Support/...`（mac）/ `~/.config/...`（Linux） |
| API 密钥 | 系统凭据库；凭据库不可用时降级明文存入 config.json（会弹出警告） |
| 最近快照 | 同目录 `snapshots.json`（用于冷启动即时显示） |

卸载并删除上述目录即可完全清除数据。

## 开发与构建

```bash
npm install
npm run dev      # 开发调试
npm run build    # 构建当前平台安装包
```

产物位置：`src-tauri/target/release/bundle/`

- Windows：NSIS 安装包 `.exe`
- macOS：`.dmg` / `.app`
- Linux：`.AppImage` / `.deb`

**跨平台打包**：推送 `v*` 标签触发 `.github/workflows/release.yml`，在三大平台原生构建并自动发布 Release（macOS 同时产出 Apple Silicon 与 Intel 两个 dmg）。

环境要求：Rust（stable + MSVC / Xcode CLT / webkit2gtk）、Node.js ≥ 18。

## 已知限制

- macOS / Linux 托盘无系统级悬停事件，弹窗为左键点击触发（Windows 悬停正常）
- 悬停弹窗的亚克力/毛玻璃背景效果依系统版本而异（Win10 1809+ / Win11 / macOS）
- 小米 Cookie 过期后需重新粘贴；火山引擎/千帆/混元暂无公开用量 API 未内置
