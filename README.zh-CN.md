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
  <a href="https://github.com/duxweb/codux/issues">反馈</a>
</p>

<p align="center">
  <a href="README.md">English</a> | 简体中文
</p>

---

![Codux](docs/images/screenshot.png)

## 为什么需要 Codux？

AI 编程时代，开发的中心正在从 IDE 转向**终端**。

Claude Code、GitHub Copilot CLI、Cursor、Aider…… 这些 AI 工具正在让终端成为你的主力开发环境。但传统终端并不是为这个时代设计的：

- **分屏和标签管理很痛苦** — 多个项目之间来回切换，效率低下
- **没有项目概念** — 无法按项目组织、快速切换，更没有状态提醒
- **Git 需要另一个软件** — 你不得不在终端、GitKraken、Tower 之间反复横跳
- **AI 用量是个黑盒** — 你不知道今天用了多少 Token，哪个模型在消耗你的额度
- **基于 Electron 的替代品太重了** — 一个终端吃掉几百 MB 内存，不应该这样

**Codux 解决了这些问题。** 一个原生 macOS 终端工作区，专为 AI CLI 工具打造 — 多项目、多分屏、内置 Git、实时 AI 用量追踪。没有 Electron，没有 WebKit。纯 SwiftUI + AppKit，快速且轻量。

## 功能

### 多项目工作区

在一个窗口中管理所有项目。每个项目拥有独立的终端会话、分屏布局和状态 — 一切自动保存和恢复。实时活动监听会追踪所有项目中 AI 工具的运行状态，任务完成时自动通知你，让你放心切换上下文，不会错过任何结果。

### 灵活的分屏

水平分屏、底部标签页、拖拽调整大小。在同一个项目中同时处理多个任务，不丢失上下文。

![Codux 分屏工作区](docs/images/screenshot.png)

### 内置 Git 面板

分支管理、暂存更改、文件差异、提交历史、远程同步 — 全部在侧栏中完成。不再需要切换到另一个 Git 客户端。

![Codux Git 面板](docs/images/git.png)

### AI 用量仪表盘

追踪终端中运行的 AI 编程工具 — Token 消耗、模型用量、工具排行、每日趋势、实时会话监控。目前支持 **Claude Code**、**Codex (OpenAI)** 和 **Gemini CLI**，更多工具持续接入中。清楚知道你的 AI 预算花在了哪里。还有一套每日等级系统，会根据你当天的使用强度实时变化：

- `待机`
- `轻量`
- `活跃`
- `专注`
- `高强度`
- `爆肝`
- `极限`
- `封神`

![Codux AI 用量面板](docs/images/ai-stats.png)

![Codux 每日等级](docs/images/level.png)

### 宠物伙伴系统

Codux 内置了一个会随着 AI 编程行为成长的桌面宠物。你可以领取宠物蛋、命名、孵化，并让它随着日常编程逐步成长和进化。它是一层轻量陪伴系统，不是独立的小游戏界面。

宠物系统目前包含：

- 领取宠物蛋与孵化
- 等级与进化阶段成长
- 智慧、躁动、夜行、耐力、共情等性格成长值
- 按物种阶段解锁的图鉴与传承历史
- 针对运行中、完成、报错、长时间工作、喝水提醒、久坐提醒、深夜工作等场景的插嘴提醒

![Codux 宠物系统](docs/images/pet.png)

### 精致美观

每一个像素都经过打磨。玻璃模糊背景、流畅的动画、精心平衡的排版和清晰的视觉层次，不打扰你的工作。深色和浅色模式都经过完整适配，不是敷衍了事。终端主题、应用图标、快捷键都可以自定义，让它成为你自己的工具。

### 原生轻量

100% SwiftUI + AppKit 构建。没有 Electron，没有 WebKit，没有隐藏的浏览器吃你的内存。秒速启动，空闲时 CPU 占用接近零，尊重你的电池。这才是 macOS 应用该有的样子。

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

## 宠物系统说明

### 它是什么

宠物系统是 Codux 内置的一层轻量陪伴玩法。你日常的 AI 编程行为会变成一条长期成长线：领取宠物蛋、完成孵化、积累成长值、解锁进化、补全图鉴，最后把满级宠物传承进历史。

### 当前宠物与进化路线

| 物种 | 基础阶段 | 路线 A | 路线 B |
|:--|:--|:--|:--|
| `墨瞳猫` | `花花` → `玄猫` → `墨瞳猫` | `书卷猫` → `墨灵` | `暗影猫` → `暗灵` |
| `锈犬` | `毛团` → `歪耳犬` → `锈犬` | `烈犬` → `艳阳` | `钢狼` → `血月` |
| `咕咕` | `啾啾` → `呆呆` → `咕咕` | `晨雀` → `烈焰` | `风雀` → `暴风` |

### 如何领取

- 打开 Codux 后，查看标题栏里的宠物按钮
- 如果还没有领取宠物，点击按钮会打开选蛋弹窗
- 选择一个宠物蛋，可以选填名字，然后确认领取
- 领取后，宠物会随着你的 AI 编程行为持续成长

### 在哪里查看

- `标题栏宠物按钮`：打开当前宠物弹窗，查看孵化、等级、成长值和快捷入口
- `宠物图鉴`：查看已解锁阶段、当前宠物详情、传承历史
- `设置 > 宠物`：配置宠物开关、静态动画、喝水提醒、久坐提醒、深夜提醒

### 入口说明

- 宠物不是独立窗口常驻，主入口是标题栏宠物按钮
- 点击标题栏宠物按钮时：
  - 未领取宠物：打开选蛋弹窗
  - 已有宠物：打开宠物弹窗
- 图鉴入口位于宠物弹窗中

### 隐藏宠物如何获取

- 领取宠物时有一个“随机蛋”选项
- 随机蛋有概率孵出隐藏宠物
- 基础概率是 `15%`
- 如果你在最近 `7` 天内活跃使用了 `2` 个或以上 AI 工具，概率会直接提升到 `50%`

### 性格维度

- `智慧`：更长、更深的请求与高上下文工作
- `躁动`：高频、快速、短周期互动
- `夜行`：夜间使用习惯
- `耐力`：长时会话与持续活跃时间
- `共情`：偏向多轮修正、持续调试、反复收敛问题的工作模式

### 提醒设置

在 **设置 > 宠物** 中可以配置：

- 喝水提醒间隔
- 久坐提醒间隔
- 深夜提醒间隔
- 宠物开关
- 静态宠物动画模式

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
