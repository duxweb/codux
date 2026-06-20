<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux AI</h1>

<p align="center">
  <b>为 AI 编程而生的高性能跨端互联终端。</b><br/>
  用 <b>Rust + GPUI</b> 原生打造，统一 Codex、Claude Code 等 8+ AI 编程 CLI——实时状态、Token 统计、本地记忆、安全 SSH，外加桌面 ⇄ 手机 ⇄ 主机端 的互联，让你随时随地接管长 agent 任务。
</p>

<p align="center">
  <a href="https://codux.dux.cn">官网</a> &middot;
  <a href="https://github.com/duxweb/codux/releases/latest">下载</a> &middot;
  <a href="https://github.com/duxweb/codux-flutter/releases">移动端</a> &middot;
  <a href="#作者微信">作者微信</a> &middot;
  <a href="https://github.com/duxweb/codux/issues">反馈</a>
</p>

<p align="center">
  <a href="README.md">English</a> | 简体中文
</p>

---

![Codux AI](docs/images/screenshot.png)

## 为什么用 Codux AI

AI 编程 CLI 很强——也极其容易失控。真正干活时，工作会散落到项目、Git worktree、终端、历史会话、Token、远程 shell，和你只记得一半的上下文里。**Codux AI 把这片混乱收进一个稳定的原生工作台，专为认真做 AI 编程的人打造。**

| AI 编程哪里容易乱 | Codux AI 给你什么 |
| :---------------- | :---------------- |
| 每个 AI CLI 各管各的状态 | 一个按项目组织的视图，统一 Codex、Claude Code、Gemini CLI、OpenCode、Kiro CLI、Kimi Code、CodeWhale、Agy。 |
| 长 agent 任务难恢复 | 实时运行状态、本地历史、会话恢复，还有跟着 worktree 走的上下文。 |
| 并行任务互相打架 | 以 worktree 为核心：每个任务保留自己的终端、Git 状态、文件和 AI 会话。 |
| Token 花销是个黑盒 | 按工具、模型、项目、worktree、日期统计用量——不用再记账。 |
| 会话之间上下文蒸发 | 本地记忆保存习惯、项目画像、模块笔记，并自动注入回支持的 CLI。 |
| 服务器连接又脆又危险 | 已保存、已测试的 SSH 配置，加一个 **凭证永不外泄** 的 `codux-ssh` 命令给 agent 用。 |
| 任务跑一半离开电脑 | 用手机经 Iroh 配对，随时随地接着控制会话。 |
| 代码在另一台机器上 | 连上一台 Codux 主机——服务器、闲置的 Mac 或 Linux——像操作本地一样驱动它的终端、Git 和 AI。 |

Codux AI **不是** 又一个编辑器。它是给重度泡在 AI 编程 CLI 里的开发者的控制台，让多项目、长会话的 agent 工作稳得住。

## 📊 监听 & 统计每一个 AI CLI

Codux 自动识别你在终端里用的 AI CLI，读取它们的本地历史，并在工具支持时帮你装好对接和记忆文件——开箱即用。

- **实时看到 agent 在干什么。** 运行中、已完成、中断、等待授权、计划更新——每个会话都绑回正确的项目和 worktree，CLI 给出任务计划时也一并显示。
- **Token 花在哪一清二楚。** 按工具、模型、项目、worktree、日期统计用量与花销，不用自己记账。

| 工具 | 实时状态 | 历史记录 | 会话恢复 | 记忆注入 |
| :--- | :----------- | :------- | :------- | :------- |
| Codex | 完整 | 完整 | 完整 | 支持 |
| Claude Code | 完整 | 完整 | 完整 | 支持 |
| Gemini CLI | 完整 | 完整 | 取决于工具 | 支持 |
| OpenCode | 完整 | 完整 | 取决于工具 | 支持 |
| Kiro CLI | 完整 | 完整 | 取决于工具 | 支持 |
| Kimi Code | 完整 | 完整 | 取决于工具 | 取决于工具 |
| CodeWhale | 完整 | 完整 | 取决于工具 | 支持 |
| Agy | 完整 | 完整 | 取决于工具 | 支持 |

`完整` 表示日常使用里这项能力完整可追踪；`取决于工具` 表示工作区和历史都保留，具体恢复行为由 CLI 自己决定。每个工具都做了深度适配，多个会话之间不会互相串味，接入新工具也很顺。

## 🔗 一套工作区，多端互联

> **Beta。** 连接主机端会先在本次版本里以 beta 形式上线——连接、配对、主机侧数据链路都还在持续测试中，可能会有粗糙的地方，欢迎反馈。

桌面端、手机、主机端互为 **peer**，经端到端加密的 Iroh 链路连通，让你随时随地接着跑长 agent 任务。

- **手机接力。** 扫码几秒配对，自动选最快路径、连不上走中继兜底。项目、终端、文件、AI 都跑在主机上，手机只负责操控；切过去也能完整看到终端历史。
- **主机端。** 把 `codux` agent 跑在服务器、闲置的 Mac 或 Linux 上，像本地一样驱动它的终端、Git、AI——所有功能都跑在该主机自己的数据上。详见 [`apps/agent/README.md`](apps/agent/README.md)。
- **会话不怕断。** 客户端断开重连后，恢复的是*同一批*终端、shell 和正在运行的 AI；凭据始终不离开主机。

## 🧠 本地长期记忆

Codux 从你的会话里提炼长期偏好、项目画像、模块笔记，过滤噪声，只把相关的注入回支持的 CLI——让上下文跨会话存活。历史和记忆从不离开你的电脑。

## 🌳 Worktree & 分屏工作流

Codux 按真实工作发生的方式建模：**项目 → Worktree / 任务 → 终端、文件、Git、AI 会话。**

- 为并行任务开 Git worktree，不让分支状态互相缠绕。
- 终端随意分屏、开标签——切换任务时，每块分屏、面板高度、当前 AI 会话、文件上下文、Git 状态都原样回来。
- AI 历史和运行状态跟随 worktree，项目级记忆保持共享。

这正是 Codux 和普通终端复用工具的根本区别：它 *知道* 每个终端属于哪个项目和 worktree，并围绕这层关系重建整个工作区。

## 🔀 Git，独立 diff 评审

在独立的对比窗口里评审 worktree 相对 base 分支的变更、合并回主线、清理完成的 worktree——全程不用离开终端。

## 📁 终端旁边就是文件

- 浏览项目文件树，在独立窗口预览 Markdown 和图片。
- **粘贴和拖拽都对 AI 友好。** 粘贴图片自动转成本地文件路径（不是一长串 base64），拖文件进来直接是能用的路径——拿去喂 AI 即用。

## 🐾 桌面宠物

可选的桌面伙伴，会随你的 AI 编程习惯成长——会对用量、提醒和 agent 活动做反应。可以从 Petdex 导入 Codex 风格的自定义宠物包，格式是扁平的 `pet.json` + `spritesheet.png`。

## 🔒 Agent 安全 SSH

让 AI 连服务器，又看不到你的密钥。用保存好、测试过的 SSH 配置加一个 `codux-ssh` 命令执行远程命令——密码和私钥永远不会暴露给 AI。

## ⚡ 原生，不是 Electron

Codux 用 **Rust + GPUI** 打造——和 [Zed](https://zed.dev) 同源的原生技术，所以终端渲染、项目切换、长时间高强度的 agent 输出都又快又稳，不臃肿、不吃内存。桌面端、移动端、主机端共用**同一套 Rust 终端内核**，每个设备渲染出的会话完全一致——覆盖 macOS、Windows 与 Linux。

## 下载

下面每个链接都**直达最新版**——点一下就下载，不用选版本、不用跳页面。

**桌面端**

| 平台 | 下载 |
| :--- | :--- |
| macOS · Apple 芯片 | [⬇ `codux-macos-aarch64.dmg`](https://github.com/duxweb/codux/releases/latest/download/codux-macos-aarch64.dmg) |
| macOS · Intel | [⬇ `codux-macos-x86_64.dmg`](https://github.com/duxweb/codux/releases/latest/download/codux-macos-x86_64.dmg) |
| Windows 11 · x64 | [⬇ `codux-windows-x86_64-setup.exe`](https://github.com/duxweb/codux/releases/latest/download/codux-windows-x86_64-setup.exe) |

macOS 打开 `.dmg` 拖进「应用程序」；Windows 双击安装。装好后打开一个项目、在终端启动 AI CLI 就行。

**主机端（无界面 · `codux-agent`）** —— Beta，随 2.0 发布

| 平台 | 下载 |
| :--- | :--- |
| macOS · Apple 芯片 | [⬇ `codux-macos-aarch64`](https://github.com/duxweb/codux/releases/latest/download/codux-macos-aarch64) |
| macOS · Intel | [⬇ `codux-macos-x86_64`](https://github.com/duxweb/codux/releases/latest/download/codux-macos-x86_64) |
| Linux · arm64 | [⬇ `codux-linux-aarch64`](https://github.com/duxweb/codux/releases/latest/download/codux-linux-aarch64) |
| Linux · x64 | [⬇ `codux-linux-x86_64`](https://github.com/duxweb/codux/releases/latest/download/codux-linux-x86_64) |
| Windows · x64 | [⬇ `codux-windows-x86_64.exe`](https://github.com/duxweb/codux/releases/latest/download/codux-windows-x86_64.exe) |

把二进制放到 `PATH` 上（命名为 `codux`），然后 `codux config` → `codux install` → `codux qrcode`。

## 主机端命令（`codux-agent`）

| 命令 | 作用 |
| :--- | :--- |
| `codux config` | 交互式初始化（设备名、中继），写入 `codux.toml`。 |
| `codux install` | 安装为开机自启服务（launchd / `systemd --user` / 任务计划程序）。 |
| `codux start` / `stop` | 前台启动 / 停止主机端。 |
| `codux status` | 是否在运行、节点 id、已配对设备数。 |
| `codux qrcode` / `link` | 显示配对二维码 / 打印配对 ticket，粘到桌面端。 |
| `codux device` | 列出已配对设备；`device:del <id>` / `device:rename <id>` / `device:clear` 管理。 |
| `codux update` | 下载、校验并替换当前二进制，再重启主机端。 |
| `codux uninstall` | 停止并移除该服务。 |

运行 `codux <命令> --help` 查看详情，或见 [`apps/agent/README.md`](apps/agent/README.md)。

## 快捷键

| 操作 | 快捷键 |
| :--- | :----- |
| 新建分屏 | `⌘T` |
| 新建标签页 | `⌘D` |
| 切换 Git 面板 | `⌘G` |
| 切换 AI 面板 | `⌘Y` |
| 切换项目 | `⌘1` – `⌘9` |

所有快捷键都能在 **设置 → 快捷键** 里自定义。

## 演示视频

GitHub README 不渲染第三方播放器，可前往 [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/) 观看演示。

## 作者微信

扫码添加作者微信，备注 Codux，邀你加入 DUXAI 交流社群。

<p align="center">
  <img src="docs/images/wechat-author.png" width="320" alt="作者微信二维码">
</p>

## 仓库结构

本仓库是 Codux monorepo：

- `apps/desktop` —— Rust + GPUI 桌面应用、runtime、资源和发布脚本。
- `apps/agent` —— 不含 GPUI 的主机端 agent，链接协议、终端核心和共享本地 PTY 驱动。
- `apps/mobile` —— Flutter 移动控制端。
- `crates/codux-protocol` —— 共享远程协议：能力、envelope DTO、传输候选、中继规则。
- `crates/codux-protocol-ffi` —— 面向 Flutter 的协议与终端核心 C ABI 绑定。
- `crates/codux-runtime-core` —— host、项目、文件、Git、worktree、上传、终端的共享 runtime domain 规则。
- `crates/codux-terminal-core` —— 共享终端会话、序列、baseline 恢复和远程 PTY 模型（纯 Rust `alacritty_terminal` 引擎）。
- `crates/codux-terminal-pty` —— 面向桌面与主机端的共享 `portable_pty` 本地 PTY 驱动。

Flutter 保留自己的原生构建系统。远程连接完全跑在共享的 Iroh 传输上。

## 开发

```bash
cargo run
```

提交变更前建议运行：

```bash
cargo check
cargo test
node apps/desktop/scripts/release/test-package-gpui.mjs
```

桌面端通过推送版本标签（如 `v2.0.0`）触发发布。发布工作流会构建原生 macOS 和 Windows 产物、发布 GitHub Release，并更新对应的自动更新通道。

## 系统要求

**桌面端**

- macOS 14.0 (Sonoma) 或更高
- Windows 11

**主机端（`codux-agent`）**

- macOS、Linux、Windows（x86_64 与 arm64）

## 反馈

发现 Bug 或有功能建议？欢迎在 [GitHub Issues](https://github.com/duxweb/codux/issues) 提出。

提交 Bug 时，推荐用 **帮助 → 导出诊断包**，把生成的 `.zip` 附上——里面有运行日志、轮转日志、性能摘要、应用状态、无效状态备份，以及可匹配到的 macOS 诊断报告。

手动日志路径：

- `~/Library/Application Support/Codux/logs/runtime-rust.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`
- `%APPDATA%\Codux\logs\runtime-rust.log`

---

## 贡献者

感谢所有为 Codux 贡献代码、Issue、测试和反馈的朋友。

<p align="center">
  <a href="https://github.com/duxweb/codux/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=duxweb/codux" alt="Codux 贡献者">
  </a>
</p>

## GitHub Star 趋势

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  本来想叫 dmux，可惜名字被占了，那就叫 Codux 吧——中文谐音刚好是「酷 Dux」。
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
