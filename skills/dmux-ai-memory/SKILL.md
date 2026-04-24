---
name: dmux-ai-memory
description: Use when editing Codux AI memory: SQLite memory storage, extraction queueing, provider selection/testing, global prompt injection, launch context files, titlebar memory status, or settings under Settings > AI > Memory.
---

# Dmux AI Memory

Use this skill before changing AI memory storage, extraction, injection, or settings.

## Core files

- `Sources/DmuxWorkspace/Models/MemoryModels.swift`
  Memory domain model: scope, tier, kind, entries, summaries, extraction tasks, status snapshots.
- `Sources/DmuxWorkspace/Services/MemoryStore.swift`
  SQLite persistence for memory entries, summaries, FTS indexing, extraction queue state, and status snapshots.
- `Sources/DmuxWorkspace/Services/MemoryCoordinator.swift`
  Queues completed sessions for extraction, selects the extraction provider, runs compaction, and updates memory status.
- `Sources/DmuxWorkspace/Services/MemoryContextService.swift`
  Builds app-private launch artifacts for memory and global prompt injection.
- `Sources/DmuxWorkspace/Services/AIProviderService.swift`
  Provider selection, headless CLI/API invocation, provider testing, and model argument conventions.
- `Sources/DmuxWorkspace/App/AppAISettings.swift`
  AI provider configuration, global prompt, and memory settings defaults.
- `Sources/DmuxWorkspace/UI/Settings/SettingsBasicPanes.swift`
  Settings > AI UI for provider config, test buttons, global prompt, and memory controls.
- `Sources/DmuxWorkspace/UI/RootTitlebarView.swift`
  Titlebar memory extraction status indicator.

## Storage and generated files

- Durable memory lives in `memory.sqlite3` under the current Codux application support folder.
- Generated launch context lives under runtime support `memory-workspaces/<project-id>/`.
- Generated files include:
  - `memory-prompt.txt`
  - `CLAUDE.md`
  - `AGENTS.md`
  - `GEMINI.md`
  - `workspace` symlink to the real project path
- Do not write user memory into the user's repository root.

## Extraction rules

- Extraction is gated by `settings.memory.enabled` and `settings.memory.automaticExtractionEnabled`.
- Only idle sessions with a completed turn and a resolvable current project are queued.
- Tasks are fingerprinted so the same transcript is not extracted repeatedly.
- Missing-project tasks should be marked done and dropped, not left as persistent failures.
- The extraction response schema is `dmux-memory-v2`:
  - `user_summary`
  - `project_summary`
  - `working_add`
  - `working_archive`
  - `merged_entry_ids`
- Do not reintroduce old response field compatibility unless a migration need is proven from current user data.

## Provider rules

- Default memory extraction provider is `automatic`.
- Automatic provider selection tries the current terminal tool's provider first, then falls back by provider priority.
- Built-in CLI providers default to blank model strings so each CLI uses its own configured default unless the user fills a model.
- Codex model override syntax is `--model=<model>`.
- Claude, Gemini, and OpenCode model override syntax is `--model <model>`.
- Provider test buttons must exercise the same provider client path used by memory extraction.
- Background extraction must resolve real user-installed CLIs without going through Codux terminal wrappers.

## Injection rules

- Global prompt and memory context are merged into generated launch artifacts.
- Claude gets `--append-system-prompt` from `memory-prompt.txt` when available.
- Claude uses `CLAUDE.md`, Codex/OpenCode use `AGENTS.md`, and Gemini uses `GEMINI.md` through the generated memory workspace.
- Generated memory should be guidance only; prompts must tell tools to prefer current repository state over stale memory.
- Do not replace live repository inspection with memory. Memory is context, not source of truth.

## Settings rules

- Settings copy should stay short and use `Localizable.xcstrings` for user-facing text.
- Global prompt editor should remain readable with the established small text size.
- The memory status indicator must surface concrete failure reasons and stay failed until a later successful/idle status clears it.

## Tests

When changing memory behavior, run or update the relevant tests:

- `swift test --filter MemoryStoreTests`
- `swift test --filter MemoryContextServiceTests`
- `swift test --filter MemoryCoordinatorTests`
- `swift test --filter AIRuntimeBridgeServiceHookConfigTests`
