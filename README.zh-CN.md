<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux</h1>

<p align="center">
  为 AI 编程而生的 macOS 原生工作站。<br/>
  原生 SwiftUI + AppKit · GPU 加速终端 · 专为 <b>Claude Code</b>、<b>Codex</b>、<b>Gemini CLI</b>、<b>OpenCode</b> 打造。
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

---

![Codux](docs/images/screenshot.png)

## 演示视频

GitHub README 不会渲染第三方 iframe 播放器，可以前往 [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/) 观看演示视频。

## 十大亮点

| # | 功能 | 一句话说明 |
|:--|:--|:--|
| 1 | **实时 AI 活动** | 每个 AI 终端的实时状态 + 系统通知，支持 Claude / Codex / Gemini / OpenCode |
| 2 | **AI 用量与会话恢复** | 按工具 / 模型 / 项目统计 Token 历史，并一键恢复任意历史会话 |
| 3 | **每日等级** | 由真实 Token 用量驱动的每日等级，精确知道今天到底干了啥 |
| 4 | **编程伙伴** | 你的 AI 编程伴侣，按编程特性成长，并时不时插嘴让你不孤单 |
| 5 | **内置 Git** | 人性化的分支、提交、推送/拉取与同步，常用 Git 操作不离开工作区 |
| 6 | **项目文件管理** | 项目级文件管理器，可编辑、可预览、可拖入终端 |
| 7 | **多项目工作区** | 每个项目最多 **6 个分屏终端** + **不限数量 Tab**，状态独立保存 |
| 8 | **三层 AI 记忆** | 用户 / 项目 / 工具三层记忆，在 Codex、Claude、Gemini、OpenCode 间共享 |
| 9 | **移动端接力** | 外出时通过手机继续之前的 AI CLI 任务 |
| 10 | **Ghostty 引擎与主题** | 集成 `ghostty` 实现 GPU 加速终端，搭配多种浅色 / 深色主题 |

### 1. 实时 AI 活动

每个跑着 AI 工具的终端都会上报真实状态 —— 思考中、等待输入、已完成、出错 —— 同时呈现在 Tab 内指示器、项目卡片以及一回合结束的系统通知里。你不再需要盯着光标发呆，Codux 会拍拍你的肩膀。

### 2. AI 用量与会话恢复

AI 面板把零散的 AI 运行整理成可用历史档案：按 工具 / 模型 / 项目 拆分的 Token 用量、每日和趋势视图，以及任意历史会话的 **一键恢复**，直接回到原工具（Claude Code / Codex / Gemini CLI / OpenCode）继续。

![Codux AI 用量面板](docs/images/ai-stats.png)

### 3. 每日等级

由真实 AI 活动驱动的轻量级每日等级。不是一堆 Token 数字，而是一张"今天"快照 —— 跑了什么、量有多大、与平时相比如何，一眼就能看懂。

![Codux 每日等级](docs/images/level.png)

### 4. 编程伙伴

一个住在标题栏、可选的电子宠物。不同的编程特性会解锁不同的成长值与发展路线，并且偶尔插一两句嘴，让长时间的 AI 编程不那么孤单。完全可选，可一键静音。

![Codux 宠物](docs/images/pet.png)

### 5. 内置 Git

不是嵌入式 WebView，而是一等公民级别的 Git 面板。分支切换/新建/重命名/删除，行级差异的暂存，完整提交历史，以及推送 / 拉取 / 同步 —— 默认值合理，冲突处理清晰。

![Codux Git 面板](docs/images/git.png)

### 6. 项目文件管理

按项目组织的原生文件管理器。可以就地编辑代码、预览图片等素材，并把任意文件直接拖进终端 —— 让 AI 工具一次拿到正确路径。

### 7. 多项目工作区

每个项目都是独立的房间。最多 **6 个分屏终端** 并行干活，6 个不够时还可以开 **不限数量的 Tab**，每个项目独立保存布局、会话、AI 工具选择，重启后状态完整保留。

### 8. 三层 AI 记忆

Codux 会从已完成的 AI 会话中提取长期记忆，保存在本地 `memory.sqlite3`，按层组织让正确的上下文在正确的时刻出现：

- **用户层** — 跨项目的长期偏好
- **项目层** — 当前仓库的约定、决策和经验教训
- **工具层** — 为 Codex、Claude Code、Gemini CLI、OpenCode 生成应用私有的启动上下文文件（`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`）

让 Codex / Claude / Gemini / OpenCode 不再每次重开会话就忘记之前的开发内容。记忆文件由 Codux 管理，不会写进你的项目目录 —— 仓库始终是事实来源。

### 9. 移动端接力

离开 Mac 也能继续。Codux Mobile 与 Mac 主机配对后，可以在手机上发起新的 AI CLI 会话、操作正在进行的会话、浏览项目文件、上传图片 —— 真正的进程仍然跑在 Mac 上，你只是在外面盯着。

| 组件 | 用途 | 下载 |
|:--|:--|:--|
| Codux Mobile | Android 移动端：配对 Mac、远程运行 AI CLI、浏览文件、上传图片 | [Mobile Releases](https://github.com/duxweb/codux-flutter/releases) |
| Codux Service | Go 编写的轻量中继服务，负责设备配对和加密 WebSocket 转发 | [Service Releases](https://github.com/duxweb/codux-service/releases) |

如果只是快速试用，可以直接在 **设置 > 远程** 中填写以下任一官网测试节点：

| 节点 | 地址 |
|:--|:--|
| 国内中继直连 | `https://codux-service.dux.plus` |
| 全球中转加速 | `https://codux-node.dux.plus` |

终端输入、输出、文件内容、项目列表和 AI 统计在 Codux macOS 与 Codux Mobile 之间端到端加密传输。中继服务只能看到 host ID、device ID、配对状态、在线状态等路由元数据，看不到解密后的终端内容。生产或长期使用建议自托管 `codux-service`。

### 10. Ghostty 引擎与主题

Codux 集成了 [`ghostty`](https://ghostty.org) 终端引擎实现 GPU 加速渲染，即使是繁忙的 AI 输出也能保持丝滑。配合精心调校的多种浅色 / 深色主题，并跟随 macOS 外观自动切换，工作区好看又快。

## 快速开始

### 使用 Homebrew 安装

```bash
brew install --cask duxweb/tap/codux
```

### 使用 Homebrew 更新

```bash
brew update
brew upgrade --cask codux
```

### 从发布包安装

1. 从 [GitHub Releases](https://github.com/duxweb/codux/releases) 或 [codux.dux.cn](https://codux.dux.cn) 下载最新版本
2. 将 Codux 拖入应用程序文件夹
3. 打开 Codux，点击 **新建项目**，选择一个目录
4. 开始输入 — 一切就绪

> **提示"无法打开，因为无法验证开发者"？**
>
> Codux 目前尚未通过 Apple 公证，macOS 可能会阻止首次启动。解决方法：
>
> ```bash
> sudo xattr -rd com.apple.quarantine /Applications/Codux.app
> ```
>
> 或者前往 **系统设置 > 隐私与安全性**，向下滚动找到 Codux 的提示，点击 **仍要打开**。

## 快捷键

| 操作 | 快捷键 |
|:--|:--|
| 新建分屏 | `⌘T` |
| 新建标签页 | `⌘D` |
| 切换 Git 面板 | `⌘G` |
| 切换 AI 面板 | `⌘Y` |
| 切换项目 | `⌘1` - `⌘9` |

所有快捷键均可在 **设置 > 快捷键** 中自定义。

## 系统要求

- macOS 14.0 (Sonoma) 或更高版本

## 反馈

发现 Bug 或有功能建议？欢迎在 [GitHub Issues](https://github.com/duxweb/codux/issues) 中提出。

提交 Bug 时最简单的方式是 `帮助 -> 导出诊断包…`，把生成的 `.zip` 直接附在 Issue 里即可。诊断包会包含运行日志、轮转日志、性能事件摘要、已保存的应用状态、无效状态备份以及 macOS 生成的相关崩溃 / 卡死 / spin 报告。

如果需要手动提取，Codux 的运行日志默认保存在：

- `~/Library/Application Support/Codux/logs/runtime.log`
- `~/Library/Application Support/Codux/logs/runtime.previous.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`

说明：

- Codux 每次启动都会清理上一轮日志，从当前会话重新开始记录
- `runtime.previous.log` 只在当前会话日志达到轮转大小后出现
- `performance-summary.json` 记录最近的性能峰值 / 主线程卡顿摘要

直接打开日志目录：

```bash
open ~/Library/Application\ Support/Codux/logs
```

如果应用启动后立刻闪退或无响应，macOS 可能会在 `~/Library/Logs/DiagnosticReports/` 生成系统崩溃报告（`Codux-*.ips` 或 `dmux-*.ips`），请附上时间最接近崩溃发生时刻的那个：

```bash
open ~/Library/Logs/DiagnosticReports
```

提交 Issue 时建议附上：macOS 版本 + Codux 版本、复现步骤、`runtime.log`、`runtime.previous.log`（如有）、`performance-summary.json`（如有）和对应的崩溃日志（如有）。

---

## GitHub Star 趋势

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  本来想叫 dmux，可惜名字被占了，那就叫 Codux 吧，中文谐音刚好是「酷 Dux」。
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
