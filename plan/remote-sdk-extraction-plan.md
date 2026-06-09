# Remote Protocol and Terminal SDK Extraction Plan

## 目标

当前 v3.1 先作为稳定基线发布。跨端互联开始前，再把远程协议、传输驱动、远程 runtime、终端 session/pty 能力整理成可抽离的内部 SDK。先在 monorepo/workspace 内形成清晰边界，跑通 Mac/Windows/Linux/Flutter 场景后，再决定是否拆成独立仓库。

## 建议模块

1. `codux-protocol`
   - envelope schema
   - protocol version and capabilities
   - pairing payloads
   - bidirectional resource subscription messages
   - baseline/delta/resync/ack payloads
   - terminal buffer chunk/snapshot payloads during v3.1 migration
   - project/file/git/worktree/terminal domain message types

2. `codux-remote-transport`
   - transport trait/factory
   - websocket relay driver
   - webrtc datachannel driver
   - future quic driver
   - path/latency/health state normalization

3. `codux-terminal-core`
   - terminal output sequence guard
   - snapshot assembler
   - remote terminal session/cache
   - viewport ownership model
   - input ack/retry model

4. `codux-remote-runtime`
   - project list and selected project state
   - bidirectional subscription model
   - terminal session map backed by local/remote pty models
   - resource model stores for file/git/worktree/project state
   - file/git/worktree runtime domain controllers
   - host runtime instance reset handling

## 最终多端互通目标

```text
Transport driver factory
  WebSocket relay / WebRTC DataChannel / future QUIC

Protocol router
  version / capabilities / envelope / seq / ack / requestId / errors

Bidirectional subscription layer
  resource.subscribe / unsubscribe / baseline / delta / resync

Runtime models and buffer pools
  TerminalSession / RemotePtySession / FileTree / GitState / ProjectState

UI renderer
  only attaches to runtime models and emits user intent
```

Mac、Windows、Linux headless、Flutter 都按 peer 处理。任意 peer 可以发布自己拥有的资源，也可以订阅对端资源。移动端当前只发布控制意图，不发布本地项目资源；桌面端和 Linux agent 发布项目、终端、文件、Git、worktree 等资源。传输驱动只负责连通性和消息收发，上层不依赖 WebSocket、WebRTC 或未来 QUIC 的具体差异。

## 当前收口任务

1. Mac host 的 `terminal.subscribe` 支持订阅后发送 terminal baseline。
2. Flutter 订阅项目或 session 时携带 baseline/resume 选项。
3. Flutter `RemotePtySession` 作为唯一远程终端数据池：baseline、分页、live delta、held buffer、seq、resync 都进入模型。
4. UI 进入项目、前台恢复、resize 只挂载或 replay 模型；不主动全量拉历史。
5. 只有无缓存、host runtime 重启、seq gap、显式 resync 时才触发 full hydrate。
6. 后续将 `terminal.subscribe + terminal.buffer` 平滑升级为通用 `resource.subscribe + resource.baseline`。

## Monorepo 迁移计划

目标目录：

```text
codux/
  apps/
    desktop/
    mobile/
    web/
    relay-server/
  crates/
    codux-protocol/
    codux-terminal-core/
    codux-remote-transport/
    codux-remote-runtime/
  docs/
  plan/
  scripts/
```

迁移原则：

- 顶层仓库统一版本、文档、计划、发布脚本和 CI。
- Cargo workspace 只包含 Rust app/crates，不把 Flutter、web、Go 服务端加入 Cargo workspace。
- Flutter、web、Go 服务端作为 `apps/*` 子项目保留各自原生构建系统。
- 先提交当前稳定链路，再迁移目录，避免把协议改动和仓库搬迁混在一个不可回滚的 diff 里。

## Platform bindings

   - desktop Rust API uses the crates directly
   - Flutter keeps Dart implementation until cross-end API stabilizes
   - later evaluate Rust FFI for shared protocol/terminal-core only, not UI

## 不马上拆独立仓库的原因

- 当前版本需要先发布稳定基线。
- 跨端互联场景还没有完全验证，过早拆仓库会固化不成熟接口。
- 独立仓库会带来版本、发布、FFI、移动端绑定、CI 成本。
- 先在 monorepo 内抽清边界，后续拆仓库更稳。

## 抽离顺序

1. 固化 v3.1 文档和测试。
2. 先把 Mac host 和 Flutter terminal 链路对齐到订阅驱动的 RemotePtySession 模型。
3. 把 protocol payload 从 host/ui 调用点继续下沉到 protocol 模块。
4. 将 terminal baseline/sequence/remote pty session 抽成平台无关核心。
5. 将 transport driver 接口和状态机稳定为可替换工厂。
6. 跨端互联接入 Linux headless host。
7. 验证 Mac/Windows/Linux/Flutter 多端互联后再评估独立仓库。

## 发布策略

- 当前版本发布小正式版。
- 后续 SDK 抽离作为 1.8.x 或 2.x 的内部架构任务。
- 独立仓库只在跨端互联稳定后执行。
