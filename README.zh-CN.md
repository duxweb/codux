<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux AI</h1>

<p align="center">
  <b>AI 编程 CLI 的工作台。</b><br/>
  把 Claude Code、Codex、Gemini CLI、OpenCode、Kiro CLI 的项目、终端、会话、Token、记忆和远程控制统一管理起来。
</p>

<p align="center">
  <a href="https://codux.dux.cn">官网</a> &middot;
  <a href="https://github.com/duxweb/codux/releases">下载</a> &middot;
  <a href="https://github.com/duxweb/codux-flutter/releases">移动端</a> &middot;
  <a href="https://github.com/duxweb/codux-service/releases">中继服务</a> &middot;
  <a href="#作者微信">作者微信</a> &middot;
  <a href="https://github.com/duxweb/codux/issues">反馈</a>
</p>

<p align="center">
  <a href="README.md">English</a> | 简体中文
</p>

---

![Codux AI](docs/images/screenshot.png)

## 为什么用 Codux AI

AI 编程工具很强，但真正干活时，项目、终端、历史会话、Token 成本和上下文很快就会散落一地。Codux AI 做的事很直接：把这些长期 AI 编程工作流收进一个稳定的桌面工作台。

| 你需要什么 | Codux AI 做什么 |
| :--------- | :-------------- |
| 一个地方管理所有 AI CLI | 在按项目组织的终端里启动和观察 Claude Code、Codex、Gemini CLI、OpenCode、Kiro CLI。 |
| 长会话不丢、不散、不靠记忆找 | 记录 AI 活动和历史会话，需要继续时一键回到原来的工具和上下文。 |
| 看清 Token 花在哪里 | 按工具、模型、项目和日期统计用量，让 AI 编程成本变成可见数据。 |
| 能跟着项目进化的记忆 | 用本地 SQLite 管理用户习惯、项目概况和模块记忆，并为支持的 CLI 注入应用托管上下文。 |
| 终端旁边就是 Git 和文件 | 看改动、暂存 diff、浏览文件、预览素材、拖路径进终端，都在同一个工作区完成。 |
| 离开电脑也能继续推进 | Codux Mobile 配对桌面端后，可以通过加密中继远程控制 AI CLI 会话。 |

Codux AI 不是要替代你的编辑器。它面向已经重度使用 AI CLI 的开发者，解决的是多项目、长会话、上下文沉淀、Token 可视化和远程接力这些真实工作流问题。

## AI 工具支持

Codux 会从集成终端中识别已支持的 CLI，读取可用的本地会话历史，并在工具支持时安装应用托管的 hook 或记忆文件。

| 工具 | 状态与历史 | 会话恢复 | 记忆 |
| :--- | :--------- | :------- | :--- |
| Claude Code | 完整 | 完整 | 支持 |
| Codex | 完整 | 完整 | 支持 |
| Gemini CLI | 完整 | 完整 | 支持 |
| OpenCode | 完整 | 完整 | 支持 |
| Kiro CLI | 完整 | 部分 | 支持 |

`完整` 表示 Codux 可以在正常终端工作流里驱动该能力。`部分` 表示工具数据足够追踪状态，但恢复行为仍取决于该 CLI 自身支持。

## 演示视频

GitHub README 不会渲染第三方 iframe 播放器，可以前往 [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/) 观看演示视频。

## 作者微信

扫码添加作者微信，备注 Codux，即可邀请加入 DUXAI 交流社群。

<p align="center">
  <img src="docs/images/wechat-author.png" width="320" alt="作者微信二维码">
</p>

## 移动端接力

Codux Mobile 和 Codux Service 是独立的远程控制栈。真正的项目和终端始终运行在桌面端，中继服务只负责转发加密流量。

- **Codux Desktop**：主桌面端，负责项目、终端、Git、统计、记忆和远程主机。
- **Codux Mobile**：Android 客户端，用来配对桌面端、远程运行 AI CLI、浏览文件和上传图片。
- **Codux Service**：Go 编写的轻量中继服务，负责设备配对和加密 WebSocket 转发。

快速试用节点：

| 节点 | 地址 |
| :--- | :--- |
| 国内中继 | `https://codux-service.dux.plus` |
| 全球中转 | `https://codux-node.dux.plus` |

终端输入、输出、文件内容、项目列表和 AI 统计都会在 Codux Desktop 与 Codux Mobile 之间端到端加密。长期使用建议自托管 [codux-service](https://github.com/duxweb/codux-service/releases)。

## 自定义宠物

Codux 内置可选桌面伙伴，会随着你的 AI 编程习惯成长。你也可以从 Petdex 导入 Codex 风格的自定义宠物包，格式是一个 `pet.json` 加一个 `spritesheet.png`。

创作者可以参考 [Codex 宠物 atlas 规范](docs/pet-codex-atlas.md)，生成兼容的 `8 x 9` 动作 atlas 并打包导入。

## 快速开始

1. 从 [GitHub Releases](https://github.com/duxweb/codux/releases) 或 [codux.dux.cn](https://codux.dux.cn) 下载 Codux。
2. 安装应用：
   - macOS：打开 `.dmg`，将 Codux 拖入应用程序文件夹。
   - Windows：运行 `.msi` 安装包。
3. 打开一个项目目录。
4. 在集成终端里启动你的 AI CLI。

推荐下载：

| 平台 | 文件 |
| :--- | :--- |
| macOS | `macos-universal-formal.dmg` |
| Windows | `windows-x86_64-msi-*.msi` |

updater 包、unsigned 包和 `latest.json` 主要用于自动更新、测试回退或自动化流程。大多数用户下载上面两个安装包之一即可。

## 开发

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

桌面端通过推送发布标签触发构建。发布工作流会构建 macOS 和 Windows 产物，发布 GitHub Release，并更新对应的自动更新通道。

## 快捷键

| 操作 | 快捷键 |
| :--- | :----- |
| 新建分屏 | `⌘T` |
| 新建标签页 | `⌘D` |
| 切换 Git 面板 | `⌘G` |
| 切换 AI 面板 | `⌘Y` |
| 切换项目 | `⌘1` - `⌘9` |

所有快捷键均可在 **设置 > 快捷键** 中自定义。

## 系统要求

- macOS 14.0 (Sonoma) 或更高版本
- Windows 11，并安装 Microsoft WebView2 Runtime

## 反馈

发现 Bug 或有功能建议？欢迎在 [GitHub Issues](https://github.com/duxweb/codux/issues) 中提出。

提交 Bug 时，推荐使用 **帮助 -> 导出诊断包**，然后把生成的 `.zip` 附到 Issue。诊断包会包含运行日志、轮转日志、性能摘要、应用状态、无效状态备份，以及可匹配到的 macOS 诊断报告。

手动日志路径：

- `~/Library/Application Support/Codux/logs/runtime.log`
- `~/Library/Application Support/Codux/logs/runtime.previous.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`
- `%APPDATA%\Codux\logs\runtime.log`

---

## GitHub Star 趋势

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  本来想叫 dmux，可惜名字被占了，那就叫 Codux 吧，中文谐音刚好是「酷 Dux」。
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
