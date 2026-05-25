<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux</h1>

<p align="center">
  集项目、终端、Git、AI 统计、记忆、移动端控制和桌面伙伴于一体的跨平台 AI 开发工作站。<br/>
  专为 <b>Claude Code</b>、<b>Codex</b>、<b>Gemini CLI</b>、<b>OpenCode</b> 和 <b>Kiro CLI</b> 打造。
</p>

<p align="center">
  <a href="https://codux.dux.cn">官网</a> &middot;
  <a href="https://github.com/duxweb/codux/releases">下载</a> &middot;
  <a href="https://github.com/duxweb/codux-flutter/releases">移动端</a> &middot;
  <a href="https://github.com/duxweb/codux-service/releases">中继服务</a> &middot;
  <a href="https://github.com/duxweb/codux/issues">反馈</a>
</p>

<p align="center">
  <a href="README.md">English</a> | 简体中文
</p>

<p align="center">
  跨平台桌面版现在位于 <code>main</code> 分支，原 macOS 版本已保留在 <code>swift-macos</code> 分支。
</p>

---

![Codux](docs/images/screenshot.png)

<table align="center">
<tr>
  <td align="center"><img src="docs/images/ai-stats.png" width="360" alt="AI 用量与会话恢复"><br/><sub>AI 用量与会话恢复</sub></td>
  <td align="center"><img src="docs/images/level.png" width="360" alt="每日等级"><br/><sub>每日等级</sub></td>
</tr>
<tr>
  <td align="center"><img src="docs/images/git.png" width="360" alt="内置 Git"><br/><sub>内置 Git</sub></td>
  <td align="center"><img src="docs/images/pet.png" width="360" alt="编程伙伴"><br/><sub>编程伙伴</sub></td>
</tr>
</table>

## 演示视频

GitHub README 不会渲染第三方 iframe 播放器，可以前往 [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/) 观看演示视频。

## 十大亮点

| #   | 功能                  | 说明                                                                                                                                                                                                                                                             |
| :-- | :-------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **实时 AI 活动**      | 每个 AI 终端的实时运行状态 + 系统通知，支持 Claude Code、Codex、Gemini CLI、OpenCode、Kiro CLI。Tab 内指示器、项目卡片、系统通知会在每一回合结束时同时点亮 —— 你不再需要盯着光标发呆。                                                                            |
| 2   | **AI 用量与会话恢复** | 按工具 / 模型 / 项目拆分的 Token 用量、每日和趋势视图，并支持任意历史会话的 **一键恢复**，直接回到原工具继续。零散的 AI 运行变成可用的历史档案。                                                                                                                 |
| 3   | **每日等级**          | 由真实 Token 用量驱动的每日等级。一张「今天」快照告诉你跑了什么、量有多大、与平时相比如何 —— 一眼看懂、造不了假。                                                                                                                                                |
| 4   | **编程伙伴**          | 标题栏上可选的电子宠物，会随着你的 AI 编程习惯一起成长。现在支持导入 Codex 标准自定义宠物，兼容 `pet.json` + `spritesheet.png` 扁平包，可从 Petdex 安装、改名、校验、领养、封存和取回，并与内置宠物使用同一套动画、气泡和成长能力。完全可选，可一键静音。        |
| 5   | **内置 Git**          | 一等公民级别的 Git 面板，不是嵌入式 WebView。分支切换 / 新建 / 重命名 / 删除，行级差异的暂存，完整提交历史，以及推送 / 拉取 / 同步 —— 默认值合理，冲突处理清晰。                                                                                                 |
| 6   | **项目文件管理**      | 按项目组织的原生文件管理器。可以就地编辑代码、预览图片等素材，并把任意文件直接拖进终端 —— 让 AI 工具一次拿到正确路径。                                                                                                                                           |
| 7   | **多项目工作区**      | 每个项目都是独立的房间 —— 最多 **6 个分屏终端** 并行干活，6 个不够时还可以开 **不限数量的 Tab**。每个项目独立保存布局、会话、AI 工具选择，重启后状态完整保留。                                                                                                   |
| 8   | **三层 AI 记忆**      | 本地 `memory.sqlite3` 从已完成会话中提取长期记忆，按 **用户 / 项目 / 工具** 三层组织。生成应用私有的 `CLAUDE.md`、`AGENTS.md`、`GEMINI.md`，让已支持的 AI CLI 不再每次重开会话就忘记之前的开发内容 —— 同时不会写进你的项目目录。                               |
| 9   | **移动端接力**        | 离开桌面端也能继续。Codux Mobile 与桌面端主机配对后，可在手机上远程驱动 AI CLI 会话，传输流量端到端加密。详情见下方 [移动端接力](#移动端接力) 章节。                                                                                                             |
| 10  | **终端引擎与主题**    | 基于 WebView 终端提供 GPU 加速渲染、分屏、标签页和多套浅色 / 深色主题，并跟随应用外观自动切换。                                                                                                                                                                  |

## AI 工具支持度

Codux 会从终端中识别已支持的 AI CLI，在工具提供 hook 机制时安装应用托管的 hook 文件，并通过读取各工具本地会话历史补充实时事件。

| 工具 | 命令别名 | 实时活动 | 历史 / Token 统计 | 会话恢复 | 记忆注入 |
| :--- | :------- | :------- | :---------------- | :------- | :------- |
| Claude Code | `claude`, `claude-code` | 完整 | 完整 | 完整 | 支持 |
| Codex | `codex` | 完整 | 完整 | 完整 | 支持 |
| Gemini CLI | `gemini` | 完整 | 完整 | 完整 | 支持 |
| OpenCode | `opencode` | 完整 | 完整 | 完整 | 支持 |
| Kiro CLI | `kiro`, `kiro-cli` | 完整 | 完整 | 部分 | 支持 |

`完整` 表示 Codux 可以在正常集成终端流程中驱动该能力。`部分` 表示工具本地数据足够支撑状态 / 历史识别，但恢复行为仍依赖该工具自身 CLI 支持。`支持` 表示 Codux 可以为该工具注入应用托管的记忆。

## 自定义宠物

Codux 可以导入与 Codex 自定义宠物一致的扁平包格式：一个 `pet.json` 清单加一个 `spritesheet.png` atlas。你可以在领养或图鉴流程里打开 Petdex 宠物市场，粘贴 Petdex 宠物页面地址，预览元数据，调整显示名称后安装到 Codux。安装后的自定义宠物会和内置宠物一起出现在领养列表里，并保留同样的领养、封存、取回、动画、气泡和成长行为。

创作者可以参考 [Codex 宠物 atlas 规范](docs/pet-codex-atlas.md)，生成兼容的 `8 x 9` 动作 atlas 并打包导入。

## 移动端接力

Codux Mobile + Codux Service 是独立的远程访问栈，中继服务可以自托管，真正的项目和终端始终运行在你的桌面端上。

| 组件          | 用途                                                            | 下载                                                                 |
| :------------ | :-------------------------------------------------------------- | :------------------------------------------------------------------- |
| Codux Desktop | 主桌面端：项目、终端、Git、AI 统计、记忆和远程主机会话          | [Desktop Releases](https://github.com/duxweb/codux/releases)         |
| Codux Mobile  | Android 移动端：配对桌面端、远程运行 AI CLI、浏览文件、上传图片 | [Mobile Releases](https://github.com/duxweb/codux-flutter/releases)  |
| Codux Service | Go 编写的轻量中继服务，负责设备配对和加密 WebSocket 转发        | [Service Releases](https://github.com/duxweb/codux-service/releases) |

如果只是快速试用，可以直接在 **设置 > 远程** 中填写以下任一官网测试节点：

| 节点         | 地址                             |
| :----------- | :------------------------------- |
| 国内中继直连 | `https://codux-service.dux.plus` |
| 全球中转加速 | `https://codux-node.dux.plus`    |

终端的输入、输出、文件内容、项目列表和 AI 统计在 Codux Desktop 与 Codux Mobile 之间端到端加密传输。中继服务只能看到 host ID、device ID、配对状态、在线状态等路由元数据，看不到解密后的终端内容。生产或长期使用建议自托管 `codux-service`。

## 快速开始

### 从发布包安装

1. 从 [GitHub Releases](https://github.com/duxweb/codux/releases) 或 [codux.dux.cn](https://codux.dux.cn) 下载最新的 macOS 或 Windows 版本
2. 安装 Codux：
   - macOS：打开 `.dmg`，将 Codux 拖入应用程序文件夹
   - Windows：运行 `.msi` 安装包
3. 打开 Codux，点击 **新建项目** 或 **打开文件夹**，选择一个目录
4. 开始输入 — 一切就绪

Codux 使用内置更新器。稳定版和测试版都从 GitHub Releases 发布，应用会按当前配置的更新通道自动检测。

### 应该下载哪个文件？

| 平台 | 文件 | 用途 |
| :--- | :--- | :--- |
| macOS | `macos-universal-formal.dmg` | 推荐下载的 macOS 安装包，已使用 Developer ID 签名并通过 Apple 公证。 |
| macOS | `macos-universal-unsigned.dmg` | 快速回退 / 测试包，首次启动时可能需要在 macOS Gatekeeper 中手动放行。 |
| macOS | `macos-universal-*-updater.app.tar.gz` | 自动更新专用包，不需要手动下载安装。 |
| Windows | `windows-x86_64-msi-*.msi` | 推荐下载的 Windows 安装包。 |
| Windows | `windows-x86_64-nsis-*.exe` | Windows 备用安装包。 |
| 全平台 | `latest.json` | 自动更新元数据，不需要手动下载。 |

如果 macOS 阻止启动 unsigned 测试包，可以前往 **系统设置 > 隐私与安全性**，找到 Codux 提示后点击 **仍要打开**，或执行：

```bash
sudo xattr -rd com.apple.quarantine /Applications/Codux.app
```

### 开发

```bash
pnpm install
pnpm tauri dev
```

提交变更前建议运行：

```bash
pnpm exec tsc --noEmit
pnpm run lint
cargo check --manifest-path src-tauri/Cargo.toml
```

### 发布

桌面端通过推送发布标签触发构建：

```bash
git tag v1.0.0-beta.1
git push origin v1.0.0-beta.1
```

发布工作流会读取标签版本，自动写入桌面端 / package 清单，从 `CHANGELOG.md` 提取对应版本更新日志，构建 macOS 与 Windows 产物，发布 GitHub Release，并更新 beta 或 stable 更新通道。

## 快捷键

| 操作          | 快捷键      |
| :------------ | :---------- |
| 新建分屏      | `⌘T`        |
| 新建标签页    | `⌘D`        |
| 切换 Git 面板 | `⌘G`        |
| 切换 AI 面板  | `⌘Y`        |
| 切换项目      | `⌘1` - `⌘9` |

所有快捷键均可在 **设置 > 快捷键** 中自定义。

## 系统要求

- macOS 14.0 (Sonoma) 或更高版本
- Windows 10 / 11，并安装 Microsoft WebView2 Runtime

## 反馈

发现 Bug 或有功能建议？欢迎在 [GitHub Issues](https://github.com/duxweb/codux/issues) 中提出。

提交 Bug 时最简单的方式是 `帮助 -> 导出诊断包…`，把生成的 `.zip` 直接附在 Issue 里即可。诊断包会包含运行日志、轮转日志、性能事件摘要、已保存的应用状态、无效状态备份以及 macOS 生成的相关崩溃 / 卡死 / spin 报告。

如果需要手动提取，Codux 的运行日志默认保存在：

- `~/Library/Application Support/Codux/logs/runtime.log`
- `~/Library/Application Support/Codux/logs/runtime.previous.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`
- `%APPDATA%\Codux\logs\runtime.log`

说明：

- Codux 每次启动都会清理上一轮日志，从当前会话重新开始记录
- `runtime.previous.log` 只在当前会话日志达到轮转大小后出现
- `performance-summary.json` 记录最近的性能峰值 / 主线程卡顿摘要

直接打开 macOS 日志目录：

```bash
open ~/Library/Application\ Support/Codux/logs
```

如果应用启动后立刻闪退或无响应，macOS 可能会在 `~/Library/Logs/DiagnosticReports/` 生成系统崩溃报告（`Codux-*.ips` 或 `dmux-*.ips`），请附上时间最接近崩溃发生时刻的那个：

```bash
open ~/Library/Logs/DiagnosticReports
```

提交 Issue 时建议附上：系统版本 + Codux 版本、复现步骤、`runtime.log`、`runtime.previous.log`（如有）、`performance-summary.json`（如有）和对应的崩溃日志（如有）。

---

## GitHub Star 趋势

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  本来想叫 dmux，可惜名字被占了，那就叫 Codux 吧，中文谐音刚好是「酷 Dux」。
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
