# AI Memory Architecture

## Flow

1. `AISessionStore` emits render changes as AI sessions move through prompt, response, input, and completion states.
2. `AppModel` passes completed session snapshots to `MemoryCoordinator`.
3. `MemoryCoordinator` resolves the project and transcript source, fingerprints it, and enqueues an extraction task in `MemoryStore`.
4. `AIProviderSelectionService` selects the configured extraction provider.
5. `AIProviderFactory` runs a headless CLI/API completion and expects `dmux-memory-v2` JSON.
6. `MemoryStore` upserts working memory and summaries, archives merged entries, and trims active working entries.
7. `MemoryContextService` renders global prompt, user summary, project summary, and recent working notes into launch artifacts for future sessions.

## Data model

- User memory: cross-project durable preferences, conventions, decisions, facts, and bug lessons.
- Project memory: repository-specific durable facts, conventions, decisions, and bug lessons.
- Working memory: fresh short-term entries before compaction.
- Summary memory: compact durable user/project summaries with version history.

## Important boundaries

- Memory extraction reads transcripts and existing memory only.
- Memory injection writes generated files into Codux runtime support, not the repository.
- Memory status is an app UI status, not a project activity/loading signal.
- Provider configuration belongs to Settings > AI, while runtime tool permissions/models/global prompt belong to tool configuration and global prompt settings.
