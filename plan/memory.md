# 项目记忆系统实施方案

## 一、目标

给 DmuxWorkspace 加一套"按项目隔离的长期记忆"能力：

- 新会话启动时自动把该项目的记忆注入 AI CLI 的系统提示
- 会话过程中 AI 可以主动记下新事实（可选，后期）
- 会话结束后自动从 transcript 提炼增量记忆
- 完全本地、按项目隔离、数据复用现有 SQLite 基础设施
- 零外部运行时依赖（无 Python、无 Node server、无新 MCP server 进程）
- 复用用户已登录的 CLI 订阅额度做记忆抽取，不依赖额外 API key

## 二、核心原理

记忆系统在本质上只有两件事：**写入**（把值得记的东西存进数据库）和**读取**（把库里相关内容塞进 LLM 的上下文）。

本方案选择的路径组合：

| 环节 | 选择 | 理由 |
|---|---|---|
| 存储 | 现有 SQLite 扩一张表 | 单一真相源，可 JOIN 现有 project / usage / activity |
| 写入方式 | 会话结束抽取（离线） + LLM 调用 CLI（在线，后期） | 先跑通离线路径，零实时复杂度 |
| 读取方式 | 启动前渲染进 CLAUDE.md / AGENTS.md / GEMINI.md | 复用 CLI 约定文件机制，无需 MCP |
| 检索 | SQLite FTS5 内置 | 零依赖，覆盖 90% 场景；后期按需加 sqlite-vec |
| 抽取 LLM | 复用用户登录的 CLI 的 headless 模式（`claude -p` / `codex exec` / `gemini -p`） | 零新成本、零新依赖 |

**不使用 MCP**——因为 MCP 的价值是"会话中给 LLM 注入可调工具"，而本方案的读取走"启动前渲染 markdown"，写入走"会话结束离线抽取"，不需要会话中实时互动的工具。后期如果要做实时记忆，用 CLI Bash 工具调一个打包在 app 里的 `dmem` 小命令就够了（仍然不需要 MCP）。

## 三、技术栈

已有、无需新增：
- SQLite3（`linkedLibrary("sqlite3")` 已在 Package.swift）
- 现有 Store 模式（`AIUsageStore` / `AIStatsStore` 等使用原生 `sqlite3_*` C API）
- 现有 hook 注入基础设施（`AIRuntimeBridgeService.ensureShellHooksStaged` / `ensureCodexHooksInstalled` / `ensureGeminiHooksInstalled`）
- 现有项目模型 `Project` (`Models/AppModels.swift`)
- App Support 根目录：`~/Library/Application Support/dmux/`

可选（后期）：
- `sqlite-vec` C 库（向量检索，超过 200 条记忆或 FTS5 召回不够时再加）
- Apple `NaturalLanguage` framework（生成本地 sentence embedding，仅向量检索启用时需要）

不使用：
- mem0 / Letta / Zep（是 Python/Node 服务，不适合桌面分发）
- MCP server（目标是零新进程）
- Ollama（用户装起来麻烦）
- 外部 API（要 key，用户体验差）

## 四、数据模型

**所有记忆表都放进现有 SQLite（或新建一个 `memory.sqlite3` 文件，路径在 `~/Library/Application Support/dmux/` 下）。推荐新建独立 db 文件，避免和 AI usage 表耦合。**

```sql
-- 核心记忆表
CREATE TABLE IF NOT EXISTS memory (
  id TEXT PRIMARY KEY,                     -- UUID
  project_id TEXT NOT NULL,                -- FK 逻辑上指向 Project.id
  tier INTEGER NOT NULL DEFAULT 2,         -- 1=核心 2=项目 3=归档
  kind TEXT NOT NULL,                      -- preference/decision/fact/bug_lesson/convention
  content TEXT NOT NULL,                   -- 一句话事实，≤300 字符
  rationale TEXT,                          -- 可选：why/上下文，用来判断是否过时
  source_session_id TEXT,                  -- 来源 session（可追溯到 transcript）
  source_tool TEXT,                        -- claude/codex/gemini/opencode
  created_at REAL NOT NULL,
  updated_at REAL NOT NULL,
  last_accessed_at REAL,
  access_count INTEGER NOT NULL DEFAULT 0,
  superseded_by TEXT REFERENCES memory(id),-- 新事实覆盖旧事实时记录
  pinned INTEGER NOT NULL DEFAULT 0        -- 用户手动置顶的不参与衰减
);

CREATE INDEX IF NOT EXISTS idx_memory_project_tier ON memory(project_id, tier);
CREATE INDEX IF NOT EXISTS idx_memory_superseded ON memory(superseded_by);

-- 全文搜索（SQLite 原生，零依赖）
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
  content, rationale,
  content='memory', content_rowid='rowid',
  tokenize='porter unicode61'
);

-- 触发器保持 FTS 同步
CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory BEGIN
  INSERT INTO memory_fts(rowid, content, rationale) VALUES (new.rowid, new.content, COALESCE(new.rationale, ''));
END;
CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE ON memory BEGIN
  INSERT INTO memory_fts(memory_fts, rowid, content, rationale) VALUES('delete', old.rowid, old.content, COALESCE(old.rationale, ''));
  INSERT INTO memory_fts(rowid, content, rationale) VALUES (new.rowid, new.content, COALESCE(new.rationale, ''));
END;
CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory BEGIN
  INSERT INTO memory_fts(memory_fts, rowid, content, rationale) VALUES('delete', old.rowid, old.content, COALESCE(old.rationale, ''));
END;

-- 抽取任务队列（SessionEnd 入队，后台 worker 消费）
CREATE TABLE IF NOT EXISTS memory_extraction_queue (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  tool TEXT NOT NULL,
  transcript_path TEXT NOT NULL,
  enqueued_at REAL NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'pending',  -- pending/running/done/failed
  error TEXT
);

-- 整合/衰减元数据
CREATE TABLE IF NOT EXISTS memory_project_meta (
  project_id TEXT PRIMARY KEY,
  last_consolidated_at REAL,
  total_count INTEGER NOT NULL DEFAULT 0,
  l1_count INTEGER NOT NULL DEFAULT 0,
  l2_count INTEGER NOT NULL DEFAULT 0
);
```

### 分层策略（tier）

- **L1 核心记忆（≤10 条）**：用户角色、项目强约束、高频偏好。**每次会话启动强制全量注入**。
- **L2 项目知识（≤200 条）**：技术决策、架构、常用命令、关键 bug 教训。启动时全量注入（<5KB）或 top-20 检索。
- **L3 归档（无限）**：低频事实、被废黜的旧记忆。不注入，仅供手动查询或后期向量检索。

条数超限触发**整合（consolidation）**：调一次 headless LLM 把相似条目合并，或把长期未访问的条目降级到 L3。

## 五、模块划分

新增文件全部放到 `Sources/DmuxWorkspace/Services/` 和 `Sources/DmuxWorkspace/Models/`：

### `Models/MemoryModels.swift`
```swift
enum MemoryTier: Int, Codable, Sendable { case core = 1, project = 2, archive = 3 }
enum MemoryKind: String, Codable, Sendable {
  case preference, decision, fact, bugLesson = "bug_lesson", convention
}

struct MemoryEntry: Identifiable, Codable, Sendable {
  let id: UUID
  var projectID: UUID
  var tier: MemoryTier
  var kind: MemoryKind
  var content: String
  var rationale: String?
  var sourceSessionID: String?
  var sourceTool: String?
  var createdAt: Date
  var updatedAt: Date
  var lastAccessedAt: Date?
  var accessCount: Int
  var supersededBy: UUID?
  var pinned: Bool
}

struct MemoryExtractionTask: Sendable {
  let id: UUID
  let projectID: UUID
  let sessionID: String
  let tool: String
  let transcriptPath: URL
  let enqueuedAt: Date
  var attempts: Int
}
```

### `Services/MemoryStore.swift`
仿 `AIUsageStore` 风格，原生 `sqlite3_*` C API，负责：
- 数据库初始化（建表、建索引、建 FTS 触发器）
- CRUD：`insert`, `update`, `delete`, `listActive(projectID:tier:)`, `search(projectID:query:)`
- Supersession：`markSuperseded(oldID:by:)`
- 统计：`countByTier(projectID:)`, `bumpAccess(id:)`

### `Services/MemoryRenderingService.swift`
负责把记忆渲染成 markdown 片段，供注入到 CLI 约定文件：

```
## Dmux Project Memory (auto-generated, do not edit)
<!-- BEGIN DMUX-MEMORY -->
### Core preferences
- [preference] 使用 tab 缩进
- [convention] 所有 SQL 用 sqlite3 C API 风格

### Project knowledge
- [decision] 2026-02: 切换到 pnpm（原因：workspace 支持更好）
- [bug_lesson] 不要在 MainActor 里跑 sqlite3_open（会阻塞 UI）
...
<!-- END DMUX-MEMORY -->
```

边界用 HTML 注释做"幂等替换锚"，每次重新渲染都只替换这两行之间的内容，不影响用户手写部分。

### `Services/MemoryInjectionService.swift`
职责：会话启动前把 markdown 写入对应 CLI 的约定文件。

每个 CLI 的目标文件（**项目级优先，全局级兜底**）：

| CLI | 项目级文件 | 全局级文件 |
|---|---|---|
| Claude Code | `<project>/CLAUDE.md` | `~/.claude/CLAUDE.md` |
| Codex CLI | `<project>/AGENTS.md` | `~/.codex/AGENTS.md` |
| Gemini CLI | `<project>/GEMINI.md` | `~/.gemini/GEMINI.md` |
| OpenCode | `<project>/AGENTS.md` | 同 Codex |

**推荐写入项目级文件**，因为这样天然按项目隔离，用户 clone 项目到别处打开也会继承记忆（前提是 git 不忽略它）。如果项目 `.gitignore` 了这些文件则继续用；如果没 gitignore，提供 UI 让用户选择"写项目级"还是"写全局但按项目名 scope"。

### `Services/MemoryExtractionService.swift`
职责：从 transcript 提炼事实，写入数据库。

**触发**：SessionEnd hook → 入队 `memory_extraction_queue` → 后台 Task 串行消费。

**抽取步骤**：
1. 从队列取一条
2. 读 transcript 文件（Claude/Codex/Gemini 各家格式不同，已有 probe service 可以复用解析）
3. 裁剪 transcript（保留最后 N 轮 + 系统提示中已注入的现有记忆）
4. Spawn headless CLI：
   ```bash
   claude -p "$EXTRACTION_PROMPT" --model claude-haiku-4-5 < transcript.txt
   # 或
   codex exec --model gpt-4o-mini "$EXTRACTION_PROMPT"
   # 或
   gemini -p "$EXTRACTION_PROMPT" --model gemini-flash
   ```
5. 解析返回的 JSON（要求 LLM 输出固定 schema：`{"add": [...], "supersede": [...], "delete": [...]}`）
6. 应用到 SQLite，去重（FTS 找相似条目，或后期 embedding 余弦 >0.95 直接替换）
7. 标记任务 done；失败最多重试 3 次。

**抽取 prompt 模板**（写进 Resources/MemoryPrompts/extract.md）：
```
你将从一段 AI 编程助手的对话记录中提炼"值得长期记住的事实"。

已有记忆（用于判断重复或冲突）：
<existing_memory>
{injected_list}
</existing_memory>

对话记录：
<transcript>
{transcript}
</transcript>

请只输出 JSON，格式如下：
{
  "add": [{"kind":"preference|decision|fact|bug_lesson|convention","content":"...","rationale":"..."}],
  "supersede": [{"old_id":"...","new_content":"...","new_rationale":"..."}],
  "delete": ["id1","id2"]
}

原则：
- 只记"跨会话仍有效"的事实（技术栈、决策、偏好、约定、教训）
- 不记当前任务本身（"修复了 X bug" 不算，除非有可复用教训）
- 每条 ≤200 字符
- 如果没有值得记的，返回 {"add":[],"supersede":[],"delete":[]}
```

### `Services/MemoryConsolidationService.swift`
定期（每天一次或记忆条数超限时）：
- 扫描同 project 下语义相似度高的条目（FTS 重叠度 + 时间接近），调 headless LLM 合并
- 把 last_accessed 超过 90 天的 L2 降级到 L3
- 清理 `superseded_by` 链上的闭环和孤儿

### `Services/MemoryHookIntegration.swift`
把上面几个 service 接到现有的 `AIRuntimeBridgeService` 的 hook 生命周期里：

1. **SessionStart**（CLI 启动前）
   - `MemoryRenderingService.render(project)` → markdown
   - `MemoryInjectionService.inject(into: projectPath, for: tool)` → 替换约定文件锚点区间
   - `MemoryStore.bumpAccess` 记录 L1 条目被读取

2. **SessionEnd**（CLI 退出 / session 关闭）
   - 找到 transcript 文件路径（各 CLI 已有 probe service 知道）
   - `MemoryStore.enqueueExtraction(project, session, transcriptPath)`
   - 异步 Task 启动 extraction worker（如果尚未运行）

3. **AppLaunch**（启动时）
   - 启动 consolidation worker（后台 Task，循环 sleep 1h 后扫一次）
   - 恢复未完成的 extraction 任务

### `Services/MemoryHeadlessLLMService.swift`
封装对"已登录 CLI 的 headless 调用"的抽象：

```swift
protocol MemoryHeadlessLLM: Sendable {
  func complete(prompt: String, systemHint: String?) async throws -> String
}

struct ClaudeHeadlessLLM: MemoryHeadlessLLM { ... }  // claude -p
struct CodexHeadlessLLM: MemoryHeadlessLLM { ... }   // codex exec
struct GeminiHeadlessLLM: MemoryHeadlessLLM { ... }  // gemini -p
```

选择策略：
1. 优先用当前项目正在用的 CLI（省切换）
2. 如果没检测到活跃 CLI，按全局顺序回退：Claude → Codex → Gemini → OpenCode
3. 模型偏好：每个 CLI 用其最便宜的小模型（Haiku / GPT-4o-mini / Gemini Flash）

### `App/MemoryAppState.swift`（UI 层）
- `@Observable` 或 `ObservableObject` 管理 memory 列表的实时视图
- 监听 `MemoryStore` 变更事件
- 驱动 UI 面板：列表 / 编辑 / 删除 / 置顶 / 手动新增

## 六、接给 CLI 的完整流程

### 启动会话
```
用户在 Dmux 点击 "打开 Claude" for Project X
  ↓
AppModel.openAISession(project: X, tool: claude)
  ↓
MemoryHookIntegration.beforeSessionStart(project: X, tool: claude)
  ↓
MemoryRenderingService.render(project: X) 
  → 查 L1+L2 记忆 → 拼 markdown
  ↓
MemoryInjectionService.inject(content, into: X.path/CLAUDE.md, mode: anchorReplace)
  ↓
现有 AIRuntimeBridgeService 正常 spawn claude CLI
  ↓
claude 读到 CLAUDE.md，记忆进入系统提示
```

### 会话结束
```
CLI 退出（或 session 被 Dmux 关闭）
  ↓
现有 hook 捕获 session end 事件
  ↓
MemoryHookIntegration.afterSessionEnd(project, session, transcriptPath)
  ↓
MemoryStore.enqueueExtraction(task)
  ↓
[后台 worker 轮询队列]
  ↓
MemoryExtractionService.run(task)
  → 读 transcript 
  → 构造 prompt 
  → 调 Haiku/Flash headless
  → 解析 JSON 
  → 去重合并写入 MemoryStore
  ↓
可选：推送 UI 通知 "从本次会话学到 3 条新记忆"
```

### 会话进行中（**暂不实现，后期选做**）
如果后期要支持"对话中 AI 主动写记忆"：
- 在 app bundle 里打包一个 Swift 编译的 `dmem` CLI（独立 target，link 同一个 MemoryStore 模块）
- 在 CLAUDE.md 尾部追加："如果用户告诉你一条值得长期记住的事实，运行 `dmem add --project <id> --kind <k> '<content>'`"
- LLM 通过已有的 Bash 工具执行 dmem，写入共享 SQLite
- **仍然不用 MCP**

## 七、记忆生命周期

```
    [transcript]
         │
         ▼
   [extraction: LLM]
         │
         ▼
   [dedup: FTS]  ── 相似度高 ──▶ [update existing]
         │
         ▼
      [insert L2]
         │
         ├── 被 search 命中 ──▶ access_count++
         │
         ├── 用户 pin  ──▶ pinned=1, 晋升 L1
         │
         ├── 90 天未访问 ──▶ 降级 L3
         │
         ├── 新事实冲突 ──▶ superseded_by=new_id
         │
         └── L2 条数 >200 ──▶ [consolidation: LLM merge]
```

## 八、UI 集成

新增面板 "项目记忆"，挂在现有项目详情旁边：
- 按 tier 分组展示（核心 / 项目 / 归档）
- 搜索框（直接接 FTS5）
- 条目操作：编辑 / 删除 / 置顶 / 降级到归档
- 手动新增按钮
- 显示最近一次抽取结果 / 下一次整合时间
- 导出按钮（接到现有 `AppDiagnosticsExportService`）

## 九、分阶段实施

### Phase 1（MVP，1-2 天）
- [ ] `MemoryModels.swift`
- [ ] `MemoryStore.swift`（纯 CRUD + FTS5）
- [ ] `MemoryRenderingService.swift`
- [ ] `MemoryInjectionService.swift`（仅处理 CLAUDE.md 一家）
- [ ] 接入 `AIRuntimeBridgeService` 的 SessionStart（仅 Claude）
- [ ] 一个极简 UI 面板（列表 + 手动增删）

验收：用户手动加 5 条记忆，下次打开 Claude 能在对话里看到模型"知道"这些事。

### Phase 2（自动抽取，2-3 天）
- [ ] `MemoryHeadlessLLMService.swift`（Claude headless 一家）
- [ ] `MemoryExtractionService.swift`
- [ ] `memory_extraction_queue` 后台 worker
- [ ] 接入 SessionEnd hook
- [ ] UI 显示"上次从会话抽到 N 条"

验收：一次真实会话结束后，几十秒内 SQLite 里出现合理的新记忆条目。

### Phase 3（多 CLI 支持，1-2 天）
- [ ] `MemoryInjectionService` 扩展到 AGENTS.md / GEMINI.md
- [ ] `MemoryHeadlessLLMService` 加 Codex / Gemini / OpenCode 实现
- [ ] 项目设置里选"偏好用哪个 CLI 做抽取"

### Phase 4（整合与衰减，2 天）
- [ ] `MemoryConsolidationService.swift`
- [ ] 去重/合并/降级/废黜逻辑
- [ ] 定时后台任务

### Phase 5（可选增强）
- [ ] 接入 `sqlite-vec` + Apple `NaturalLanguage` embedding
- [ ] `dmem` CLI + 在线记忆写入
- [ ] 跨项目"全局记忆"池（用户偏好级）
- [ ] Git-aware：记忆绑到 commit，checkout 切换时切换记忆视图

## 十、关键设计决策记录

1. **为什么不用 MCP**：读取走 markdown 注入、写入走会话后离线抽取，都不需要 LLM 在会话中调外部工具。MCP 会引入新协议层和子进程生命周期管理，对纯桌面软件是负担。
2. **为什么不用 mem0 等现成框架**：它们是 Python/Node 服务，分发体验差；自己用 SQLite+FTS5 实现 200 行 Swift 代码能覆盖 90% 功能，并且数据模型完全可控。
3. **为什么用 headless CLI 而不是 API**：用户已经在用 Claude Pro / ChatGPT Plus 等订阅，这些订阅包月，额度基本用不完；直接复用等于零新成本、零新依赖、零配置。用户不需要再去搞 API key。
4. **为什么用独立的 memory.sqlite3**：虽然理论上可以塞进 `ai-usage.sqlite3`，但记忆系统的备份/导出/清理节奏和使用量统计不同，独立文件更利于单独迁移和重置。
5. **为什么优先写项目级约定文件**：天然按项目隔离，且用户手动查看/编辑方便；跨机器同步通过 git 完成。只有当用户显式不想污染项目仓库时才用全局 + scope。
6. **为什么先用 FTS5 不用向量**：FTS5 是 SQLite 内置、零依赖、对 <200 条记忆的召回完全够用；向量检索的价值在规模大到无法全量注入时才显现——那是 Phase 5 之后的事。

## 十一、风险与缓解

| 风险 | 缓解 |
|---|---|
| LLM 抽取结果噪声高（记了一堆没用的） | Prompt 明确"跨会话仍有效"；UI 允许用户批量清理；抽取结果默认进 L2 而非 L1 |
| CLAUDE.md 被用户手写内容污染冲突 | 用 HTML 注释锚 `<!-- BEGIN/END DMUX-MEMORY -->` 做幂等替换，不碰锚外内容 |
| 多会话并发写库 | SQLite WAL 模式；extraction 串行消费队列；写入包事务 |
| transcript 解析各 CLI 格式不一 | 复用现有 `*RuntimeProbeService` 的解析逻辑，不重复造轮子 |
| headless LLM 调用失败 / 超时 | 队列有 attempts 字段，最多重试 3 次；失败任务保留，用户可手动重试 |
| 用户隐私顾虑 | 全本地存储；设置里提供"关闭自动抽取"开关；提供"清空项目记忆"按钮 |
| 记忆膨胀导致上下文爆炸 | L1 硬上限 10 条 / L2 硬上限 200 条；超限触发 consolidation |

## 十二、非目标

为了控制范围，**本方案明确不做**：

- 跨项目记忆同步（云同步）——用户需要可通过 git 项目级文件或手动导出解决
- 多用户共享记忆——单用户桌面应用，不考虑
- 实时推荐"相关记忆"到当前对话——先做好基础注入，再考虑
- 图形化知识图谱——数据模型预留了 kind / rationale / superseded_by，后期可视化是 UI 层的事
- 记忆的加密——SQLite 文件依赖 macOS 磁盘加密即可
