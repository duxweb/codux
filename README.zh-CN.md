<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux</h1>

<p align="center">
  一个为 AI 编程工具打造的原生 macOS 终端工作区。
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

<div align="center">
  <iframe src="https://player.bilibili.com/player.html?isOutside=true&aid=116493605935938&bvid=BV1mK9vBCEYD&cid=37979163134&p=1" width="100%" height="420" scrolling="no" border="0" frameborder="no" framespacing="0" allowfullscreen="true"></iframe>
</div>

如果内嵌播放器无法显示，可以前往 [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/) 观看。

## 为什么需要 Codux？

Codux 面向主要使用 Claude Code、Codex、Gemini CLI、OpenCode 等终端 AI 编程工具的开发者。

| 使用痛点 | Codux 提供的能力 |
|:--|:--|
| 多个项目的终端窗口越来越乱 | 按项目组织的工作区，每个项目保留自己的会话、分屏布局和状态 |
| AI 任务在别的项目里跑完了你不知道 | 项目活动状态和通知会显示运行中、等待输入、已完成等状态 |
| Git 还要打开另一个客户端 | 内置 Git 面板支持分支、差异、暂存、提交历史和远程同步 |
| AI Token 用量看不清楚 | 实时统计工具、模型、请求、Token 和每日趋势 |
| 有用的 AI 决策埋在历史里 | 本地 AI 记忆会从已完成会话中提取偏好、项目约定和问题经验 |
| 想在手机上查看或操作 Mac 上的任务 | Codux Mobile 通过 Codux Service 连接 Mac 主机，安全运行远程终端会话 |
| Electron 终端太重 | Codux 使用原生 SwiftUI + AppKit，没有 Electron，也没有 WebKit 终端层 |

## 核心功能

| 模块 | 能力 | 入口 |
|:--|:--|:--|
| 多项目终端工作区 | 每个项目独立终端、标签、分屏、状态恢复和活动状态 | 主窗口 |
| 内置 Git | 分支、暂存、差异、提交历史和远程同步 | 侧栏 Git 面板 |
| AI 用量仪表盘 | Token 总量、模型拆分、工具排行、每日趋势和实时会话状态 | AI 面板 |
| AI 记忆 | 本地 `memory.sqlite3`、自动提取、供应商测试、全局提示词和应用私有启动上下文 | 设置 > AI 与标题栏记忆状态 |
| 远程工作区 | 配对移动设备、在 Mac 主机上创建移动端专属终端、浏览文件、上传图片、远程操作终端 | 设置 > 远程 与 Codux Mobile |
| 每日等级 | 从 `待机` 到 `封神` 的轻量每日使用强度等级 | 标题栏 / AI 统计 |
| 宠物伙伴 | 可选的轻量陪伴系统，会随 AI 编程活动成长并显示场景提醒 | 标题栏宠物按钮与设置 > 宠物 |

## 截图

| 工作区 | Git | AI 用量 |
|:--|:--|:--|
| ![Codux 分屏工作区](docs/images/screenshot.png) | ![Codux Git 面板](docs/images/git.png) | ![Codux AI 用量面板](docs/images/ai-stats.png) |

| 每日等级 | 宠物 |
|:--|:--|
| ![Codux 每日等级](docs/images/level.png) | ![Codux 宠物系统](docs/images/pet.png) |

## 远程访问

Codux 远程访问由三部分组成。这样可以让中继服务自托管，而真正的项目和终端仍然运行在你的 Mac 上。

| 组件 | 用途 | 下载 |
|:--|:--|:--|
| Codux macOS | 主桌面端，负责项目、终端、Git、AI 统计、记忆和远程主机会话 | [macOS Releases](https://github.com/duxweb/codux/releases) |
| Codux Mobile | Android 移动端，用于配对 Mac、打开远程终端、浏览项目文件和上传图片 | [Mobile Releases](https://github.com/duxweb/codux-flutter/releases) |
| Codux Service | Go 编写的轻量中继服务，负责设备配对和加密 WebSocket 消息转发 | [Service Releases](https://github.com/duxweb/codux-service/releases) |

如果只是快速试用，可以直接在 **设置 > 远程** 中填写以下任一官网测试节点：

| 节点 | 地址 |
|:--|:--|
| 国内中继直连 | `https://codux-service.dux.plus` |
| 全球中转加速 | `https://codux-node.dux.plus` |

生产环境或长期使用建议部署自己的 `codux-service`。

### 远程配置流程

| 步骤 | 操作 |
|:--|:--|
| 1 | 使用上方任一官网测试节点，或在自己的服务器、VPS、局域网机器上部署 `codux-service` |
| 2 | 生产环境建议在服务前面配置 HTTPS/WSS。Edge/CDN 可以做 WebSocket 代理，但 Go 中继本身应作为常驻进程运行 |
| 3 | 打开 Codux macOS，进入 **设置 > 远程**，填写中继服务地址并启用远程访问 |
| 4 | 点击配对按钮，显示一次性二维码 |
| 5 | 安装 Codux Mobile，扫码后核对匹配码，并在 macOS 端确认设备 |
| 6 | 在 Codux Mobile 中打开远程终端。终端实际运行在 Mac 主机上，但不会插入 Mac 当前可见的分屏布局 |

远程终端输入、输出、文件内容、项目列表和 AI 统计会在 Codux macOS 与 Codux Mobile 之间以端到端加密 payload 传输。中继服务只能看到 host ID、device ID、配对状态、在线状态等路由元数据，看不到解密后的终端内容。

## AI 记忆

Codux 可以从已完成的 AI 编程会话中提取长期记忆，把用户偏好、项目约定、技术决策、事实信息和问题经验保存到本地 SQLite 数据库中。之后启动 Claude、Codex、Gemini、OpenCode 等受支持工具时，Codux 会生成简洁上下文并注入给工具使用，不会把记忆文件写进你的项目根目录，当前仓库仍然是事实来源。

| 能力 | 说明 |
|:--|:--|
| 存储 | 使用本地 `memory.sqlite3` 保存用户记忆和项目记忆 |
| 提取 | 可自动从已完成的 Claude、Codex、Gemini、OpenCode 会话中提取摘要 |
| 供应商控制 | 支持自动供应商选择，并在 **设置 > AI** 中提供每个供应商的测试按钮 |
| 启动上下文 | 为受支持工具生成应用私有的 `CLAUDE.md`、`AGENTS.md`、`GEMINI.md` |
| 状态 | 标题栏显示记忆提取的排队、运行和失败状态 |

## 宠物伙伴

宠物系统是可选的轻量陪伴功能。它会把 AI 编程活动转成一个小型成长循环，但不会变成独立游戏界面。

| 项目 | 说明 |
|:--|:--|
| 入口 | 点击标题栏宠物按钮。没有宠物时会打开选蛋弹窗 |
| 成长 | AI 编程活动会影响孵化、等级、性格、进化、图鉴解锁和传承历史 |
| 提醒 | 喝水、久坐、深夜提醒可以在 **设置 > 宠物** 中配置 |
| 控制 | 可以在 **设置 > 宠物** 中关闭宠物，或切换为静态动画模式 |

## 原生 macOS 体验

Codux 使用 SwiftUI + AppKit 构建。启动快、空闲占用低，支持深色/浅色模式，并把终端、Git、AI 统计、记忆和远程状态放在一个原生工作区里。

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

提交 Bug 时，建议尽量附上下面这些诊断信息：

### 应用日志

最简单的方式是：

- 打开 `帮助 -> 导出诊断包…`
- 保存生成的 `.zip` 诊断包
- 提交 GitHub Issue 时直接附上这个压缩包

诊断包会尽量包含排查问题所需的关键文件，包括：

- 应用运行日志
- 历史轮转日志
- 当前会话的性能事件摘要
- 已保存的应用状态文件
- 无效状态备份文件（如果存在）
- macOS 生成的相关崩溃 / 卡死 / spin 报告（如果存在）

如果你需要手动提取，Codux 的运行日志默认保存在：

- `~/Library/Application Support/Codux/logs/runtime.log`
- `~/Library/Application Support/Codux/logs/runtime.previous.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`

说明：

- Codux 每次启动都会清理上一轮应用会话的日志，然后从当前会话重新开始记录
- `runtime.previous.log` 只会在当前会话日志达到轮转大小后出现
- `performance-summary.json` 会记录当前会话最近的性能峰值 / 主线程卡顿摘要

也可以在应用顶部栏点击 `Debug Log` 直接打开当前日志文件。

或者直接在终端执行下面这条命令，打开日志目录：

```bash
open ~/Library/Application\ Support/Codux/logs
```

### 崩溃日志

如果应用是卡死、无响应或闪退，仍然建议优先使用：

- `帮助 -> 导出诊断包…`

如果应用崩溃，或启动后立刻无响应，macOS 通常会在这里生成系统崩溃报告：

- `~/Library/Logs/DiagnosticReports/`

通常你需要找的就是下面这两类文件：

- `Codux-YYYY-MM-DD-*.ips`
- `dmux-YYYY-MM-DD-*.ips`

如果同一时间有多个文件，请优先提交时间最接近崩溃发生时刻的那个。

也可以直接在终端执行下面这条命令，打开崩溃日志目录：

```bash
open ~/Library/Logs/DiagnosticReports
```

### 建议一并附上的内容

请尽量附上或填写：

1. 你的 macOS 版本和 Codux 版本
2. 问题复现步骤
3. `runtime.log`
4. 如果存在，也请附上 `runtime.previous.log`
5. 如果存在，也请附上 `performance-summary.json`
6. 如果发生了崩溃，再附上 `~/Library/Logs/DiagnosticReports/` 中对应的崩溃日志

如果方便，建议将这些文件打包成一个 `.zip` 后再提交 Issue。

---

## GitHub Star 趋势

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  本来想叫 dmux，可惜名字被占了，那就叫 Codux 吧，中文谐音刚好是「酷 Dux」。
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
