import { useCallback, useEffect, useMemo, useState } from "react";
import {
  archiveMemoryEntry,
  deleteMemoryEntry,
  deleteProjectMemory,
  deleteMemorySummary,
  indexMemoryNow,
  migrateProjectMemory,
  readMemoryManagerSnapshot,
  updateMemorySummary,
  type MemoryEntry,
  type MemoryKind,
  type MemoryManagerSnapshot,
  type MemoryManagerTab,
  type MemoryManagerTargetRow,
  type MemoryScope,
  type MemorySummary,
} from "../ai/memory";
import { ListBox, Modal, Select as HeroSelect } from "@heroui/react";
import { Button } from "../components/Button";
import { PressableButton } from "../components/PressableButton";
import { useRuntimeStore } from "../runtimeStore";
import { FileArchive, Folder, GitBranch, PencilSquare, RefreshCw, Trash, Users, Zap, type AppIcon } from "../icons";
import { formatI18n, tm } from "../i18n";
import { flushAppSettings, readAppSettings } from "../settings";
import { systemConfirm, systemMessage } from "../systemDialog";
import { revealCurrentAppWindow } from "../windowing";
import { startWindowDrag } from "../windowDrag";
import { WindowFrame } from "./WindowFrame";

type Target = {
  scope: MemoryScope;
  projectId?: string | null;
};

const tabs: Array<{ id: MemoryManagerTab; label: string }> = [
  { id: "summary", label: tm("memory.manager.tab.summary", "Summary") },
  { id: "active", label: tm("memory.manager.tab.active", "Memories") },
  { id: "history", label: tm("memory.manager.tab.history", "History") },
];

const kindOrder: MemoryKind[] = ["preference", "convention", "decision", "fact", "bug_lesson"];

const kindColor: Record<MemoryKind, string> = {
  preference: "#8C6FF7",
  convention: "#2F7FBD",
  decision: "#B8781D",
  fact: "#337A6B",
  bug_lesson: "#C25555",
};

const tierColor: Record<string, string> = {
  core: "#3D80FA",
  working: "#2E9B5F",
  archive: "#7B8190",
};

const statusColor: Record<string, string> = {
  active: "#2E9B5F",
  merged: "#6E6E8B",
  archived: "#7B8190",
};

export function MemoryManagerWindow() {
  const [target, setTarget] = useState<Target>({ scope: "user", projectId: null });
  const [tab, setTab] = useState<MemoryManagerTab>("summary");
  const [snapshot, setSnapshot] = useState<MemoryManagerSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setLoading] = useState(true);
  const [isIndexingNow, setIndexingNow] = useState(false);
  const [editingSummary, setEditingSummary] = useState<MemorySummary | null>(null);
  const [migrationSource, setMigrationSource] = useState<MemoryManagerTargetRow | null>(null);
  const cachedSnapshot = useRuntimeStore((state) => state.memoryManagerSnapshot);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await readMemoryManagerSnapshot({
        scope: target.scope,
        projectId: target.projectId,
        tab,
        limit: 500,
      });
      useRuntimeStore.getState().setMemoryManagerSnapshot(next);
      setSnapshot(next);
      if (
        target.scope === "project" &&
        target.projectId &&
        !next.targetRows.some((row) => row.scope === "project" && row.projectId === target.projectId)
      ) {
        setTarget({ scope: "user", projectId: null });
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [tab, target.projectId, target.scope]);

  useEffect(() => {
    void load().finally(() => revealCurrentAppWindow());
  }, [load]);

  useEffect(() => {
    if (!cachedSnapshot) return;
    setSnapshot(cachedSnapshot);
  }, [cachedSnapshot]);

  const overview = snapshot?.currentOverview;
  const isIndexing = snapshot?.extraction.status === "queued" || snapshot?.extraction.status === "processing";
  const selectedProjectId = target.scope === "project" ? (target.projectId ?? null) : null;
  const canDeleteProjectMemory = Boolean(
    selectedProjectId &&
    overview &&
    overview.activeEntryCount + overview.archivedEntryCount + overview.mergedEntryCount + overview.summaryCount > 0,
  );
  const selectedTargetRow =
    snapshot?.targetRows.find(
      (row) => row.scope === target.scope && (row.projectId ?? null) === (target.projectId ?? null),
    ) ?? null;
  const canMigrateProjectMemory = Boolean(
    selectedTargetRow?.scope === "project" && selectedTargetRow.projectId && selectedTargetRow.count > 0,
  );

  return (
    <WindowFrame
      title={tm("memory.manager.window.title", "Memory Manager")}
      mainClassName="px-0 py-0"
      mainScrollable={false}
    >
      <div className="grid h-full min-h-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden bg-surface-main">
        <aside className="min-h-0 min-w-0 overflow-hidden border-r border-border-subtle/60 bg-fill/[0.025]">
          <div className="flex h-full min-w-0 flex-col">
            <div className="px-4 pb-4 pt-5">
              <div className="flex items-center gap-2">
                <div className="min-w-0 flex-1 truncate text-[17px] font-bold">
                  {tm("memory.manager.title", "Memory")}
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  isIconOnly
                  disabled={isIndexing || isIndexingNow}
                  aria-label={tm("memory.manager.index_now", "Index Now")}
                  onPress={() => void indexNow(load, setIndexingNow)}
                >
                  <Zap size={13} className={isIndexing || isIndexingNow ? "motion-safe:animate-spin" : ""} />
                </Button>
              </div>
              <div className="mt-1 text-xs leading-relaxed text-ink-mute">
                {tm("memory.manager.subtitle", "Browse and clean extracted memories")}
              </div>
            </div>

            <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden scrollbar-overlay px-2 pb-3">
              <div className="grid min-w-0 gap-1.5">
                {(snapshot?.targetRows ?? fallbackTargets()).map((row) => (
                  <TargetRow
                    key={row.id}
                    row={row}
                    selected={row.scope === target.scope && (row.projectId ?? null) === (target.projectId ?? null)}
                    onSelect={() => setTarget({ scope: row.scope, projectId: row.projectId ?? null })}
                  />
                ))}
              </div>
            </div>
          </div>
        </aside>

        <section className="flex min-h-0 min-w-0 flex-col">
          <header
            className="drag-region border-b border-border-subtle/60 px-5 pb-4 pt-5"
            data-tauri-drag-region
            onPointerDownCapture={startWindowDrag}
          >
            <div className="flex items-start gap-3">
              <div className="min-w-0 drag-region" data-tauri-drag-region>
                <h1 className="truncate text-[20px] font-bold leading-tight drag-region" data-tauri-drag-region>
                  {snapshot?.selectedTargetTitle ?? tm("memory.manager.user_memory", "User Memory")}
                </h1>
                <p className="mt-1 text-xs text-ink-mute drag-region" data-tauri-drag-region>
                  {overview
                    ? formatI18n(
                        tm("memory.manager.overview_format", "%lld active, %lld archived, %lld summaries"),
                        overview.activeEntryCount,
                        overview.archivedEntryCount + overview.mergedEntryCount,
                        overview.summaryCount,
                      )
                    : tm("memory.manager.empty.entries", "No memories in this view")}
                </p>
              </div>
              <div className="no-drag ml-auto flex items-center gap-2">
                {selectedProjectId ? (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      isIconOnly
                      disabled={!canMigrateProjectMemory}
                      aria-label={tm("memory.manager.migrate_project", "Rebind Project Memory")}
                      onPress={() => selectedTargetRow && setMigrationSource(selectedTargetRow)}
                    >
                      <GitBranch size={14} />
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      isIconOnly
                      disabled={!canDeleteProjectMemory}
                      aria-label={tm("memory.manager.delete_project", "Delete Project Memory")}
                      onPress={() => void confirmDeleteProjectMemory(selectedProjectId, load)}
                    >
                      <Trash size={14} />
                    </Button>
                  </>
                ) : null}
                <Button
                  size="sm"
                  variant="ghost"
                  isIconOnly
                  aria-label={tm("common.refresh", "Refresh")}
                  onPress={() => void load()}
                >
                  <RefreshCw size={14} />
                </Button>
              </div>
            </div>

            <div className="no-drag mt-4 inline-flex rounded-[9px] border border-border-subtle bg-fill/[0.04] p-0.5">
              {tabs.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={`h-7 rounded-[7px] px-3 text-[12px] font-semibold transition-colors ${
                    tab === item.id ? "bg-surface-main text-ink shadow-sm" : "text-ink-mute hover:text-ink"
                  }`}
                  onClick={() => setTab(item.id)}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </header>

          <main className="min-h-0 flex-1 overflow-y-auto scrollbar-overlay bg-fill/[0.012]">
            {error ? (
              <EmptyState title={tm("memory.manager.error", "Memory could not be loaded")} detail={error} />
            ) : isLoading && !snapshot ? (
              <EmptyState
                title={tm("common.loading", "Loading")}
                detail={tm("memory.manager.subtitle", "Browse and clean extracted memories")}
              />
            ) : tab === "summary" ? (
              <SummaryList
                summaries={snapshot?.summaries ?? []}
                onEdit={setEditingSummary}
                onDelete={(summary) => void confirmDeleteSummary(summary, load)}
              />
            ) : (
              <EntryList
                tab={tab}
                entries={snapshot?.entries ?? []}
                onArchive={(entry) => void archiveEntry(entry, load)}
                onDelete={(entry) => void confirmDeleteEntry(entry, load)}
              />
            )}
          </main>
        </section>
      </div>

      {editingSummary && (
        <SummaryEditor
          summary={editingSummary}
          onClose={() => setEditingSummary(null)}
          onSaved={() => {
            setEditingSummary(null);
            void load();
          }}
        />
      )}
      {migrationSource && snapshot ? (
        <ProjectMemoryMigrationDialog
          source={migrationSource}
          rows={snapshot.targetRows}
          onClose={() => setMigrationSource(null)}
          onMigrated={(projectId) => {
            setMigrationSource(null);
            setTarget({ scope: "project", projectId });
          }}
        />
      ) : null}
    </WindowFrame>
  );
}

function TargetRow({
  row,
  selected,
  onSelect,
}: {
  row: MemoryManagerTargetRow;
  selected: boolean;
  onSelect: () => void;
}) {
  const Icon = row.scope === "user" ? Users : Folder;
  return (
    <PressableButton
      className={`flex min-h-[54px] min-w-0 w-full items-center gap-2.5 overflow-hidden rounded-[8px] px-3 text-left transition-colors ${
        selected
          ? "border border-brand-blue/20 bg-brand-blue/10 text-ink"
          : "border border-transparent text-ink-soft hover:bg-fill/[0.06]"
      }`}
      onPressUp={onSelect}
    >
      <Icon size={17} className={`shrink-0 ${selected ? "text-brand-blue" : "text-ink-mute"}`} />
      <div className="min-w-0 flex-1 overflow-hidden">
        <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-semibold">
          {localizedTargetTitle(row)}
        </div>
        <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-ink-mute">
          {localizedTargetSubtitle(row)}
        </div>
      </div>
      <span
        className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-semibold tabular-nums ${selected ? "bg-brand-blue/12 text-brand-blue" : "bg-fill/[0.06] text-ink-mute"}`}
      >
        {row.count}
      </span>
    </PressableButton>
  );
}

function SummaryList({
  summaries,
  onEdit,
  onDelete,
}: {
  summaries: MemorySummary[];
  onEdit: (summary: MemorySummary) => void;
  onDelete: (summary: MemorySummary) => void;
}) {
  if (summaries.length === 0) {
    return (
      <EmptyState
        title={tm("memory.manager.empty.summary", "No summary memory")}
        detail={tm(
          "memory.manager.empty.summary.detail",
          "Summaries appear after extraction has enough useful context.",
        )}
      />
    );
  }
  return (
    <div className="grid gap-3 p-4">
      {summaries.map((summary) => (
        <article key={summary.id} className="rounded-[8px] border border-border-subtle/70 bg-surface-main p-3.5">
          <div className="flex items-start gap-2">
            <Badge
              text={formatI18n(tm("memory.manager.summary.version_format", "v%lld"), summary.version)}
              color="#3D80FA"
            />
            <Badge
              text={formatI18n(tm("memory.manager.summary.tokens_format", "%lld tokens"), summary.tokenEstimate)}
              color="#7B8190"
            />
            <div className="ml-auto flex items-center gap-1">
              <span className="mr-1 text-[11px] text-ink-faint">{formatDate(summary.updatedAt)}</span>
              <IconButton
                label={tm("memory.manager.edit_summary", "Edit Summary")}
                icon={PencilSquare}
                onPress={() => onEdit(summary)}
              />
              <IconButton label={tm("common.delete", "Delete")} icon={Trash} danger onPress={() => onDelete(summary)} />
            </div>
          </div>
          <p className="mt-3 whitespace-pre-wrap text-[13px] leading-relaxed text-ink">{summary.content}</p>
          {summary.sourceEntryIds.length > 0 && (
            <p className="mt-3 text-[11px] text-ink-mute">
              {formatI18n(
                tm("memory.manager.summary.sources_format", "%lld source entries"),
                summary.sourceEntryIds.length,
              )}
            </p>
          )}
        </article>
      ))}
    </div>
  );
}

function EntryList({
  tab,
  entries,
  onArchive,
  onDelete,
}: {
  tab: MemoryManagerTab;
  entries: MemoryEntry[];
  onArchive: (entry: MemoryEntry) => void;
  onDelete: (entry: MemoryEntry) => void;
}) {
  const groups = useMemo(
    () =>
      kindOrder
        .map((kind) => ({ kind, entries: entries.filter((entry) => entry.kind === kind) }))
        .filter((group) => group.entries.length > 0),
    [entries],
  );
  if (entries.length === 0) {
    return (
      <EmptyState
        title={
          tab === "history"
            ? tm("memory.manager.empty.history", "No memory history")
            : tm("memory.manager.empty.active", "No active memories")
        }
        detail={
          tab === "history"
            ? tm(
                "memory.manager.empty.history.detail",
                "Merged and archived memories appear here after extraction compacts older entries.",
              )
            : tm(
                "memory.manager.empty.active.detail",
                "Fresh extracted memories appear here before they are compacted into summaries. Older compacted items remain in History.",
              )
        }
      />
    );
  }
  return (
    <div className="grid gap-5 p-4">
      {groups.map((group) => (
        <section key={group.kind} className="grid gap-2.5">
          <div className="flex items-center gap-2">
            <span className="h-2 w-2 rounded-full" style={{ backgroundColor: kindColor[group.kind] }} />
            <span className="text-[12px] font-semibold text-ink-soft">{kindTitle(group.kind)}</span>
            <span
              className="rounded-full px-1.5 py-0.5 text-[11px] font-semibold"
              style={{ color: kindColor[group.kind], backgroundColor: `${kindColor[group.kind]}1c` }}
            >
              {group.entries.length}
            </span>
          </div>
          <div className="grid gap-3">
            {group.entries.map((entry) => (
              <EntryCard key={entry.id} entry={entry} onArchive={onArchive} onDelete={onDelete} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function EntryCard({
  entry,
  onArchive,
  onDelete,
}: {
  entry: MemoryEntry;
  onArchive: (entry: MemoryEntry) => void;
  onDelete: (entry: MemoryEntry) => void;
}) {
  return (
    <article className="rounded-[8px] border border-border-subtle/70 bg-surface-main p-3.5">
      <div className="flex items-start gap-2">
        <Badge text={kindTitle(entry.kind)} color={kindColor[entry.kind]} />
        <Badge text={tierTitle(entry.tier)} color={tierColor[entry.tier]} />
        <Badge text={statusTitle(entry.status)} color={statusColor[entry.status]} />
        {entry.sourceTool && <Badge text={entry.sourceTool} color="#7B8190" />}
        <div className="ml-auto flex items-center gap-1">
          <span className="mr-1 text-[11px] text-ink-faint">{formatDate(entry.updatedAt)}</span>
          {entry.status === "active" && (
            <IconButton
              label={tm("memory.manager.archive", "Archive")}
              icon={FileArchive}
              onPress={() => onArchive(entry)}
            />
          )}
          <IconButton label={tm("common.delete", "Delete")} icon={Trash} danger onPress={() => onDelete(entry)} />
        </div>
      </div>
      <p className="mt-3 whitespace-pre-wrap text-[13px] leading-relaxed text-ink">{entry.content}</p>
      {entry.rationale && (
        <p className="mt-2 whitespace-pre-wrap text-[12px] leading-relaxed text-ink-mute">{entry.rationale}</p>
      )}
    </article>
  );
}

function SummaryEditor({
  summary,
  onClose,
  onSaved,
}: {
  summary: MemorySummary;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [draft, setDraft] = useState(summary.content);
  const [isSaving, setSaving] = useState(false);
  const trimmed = draft.trim();
  const canSave = trimmed.length > 0 && trimmed !== summary.content;
  const settings = readAppSettings();

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      await updateMemorySummary({
        summaryId: summary.id,
        content: trimmed,
        maxVersions: settings.ai.memory.maxSummaryVersions,
      });
      onSaved();
    } catch (error) {
      await systemMessage(error instanceof Error ? error.message : String(error), {
        title: tm("memory.manager.error", "Memory could not be loaded"),
        kind: "warning",
        buttons: { ok: "OK" },
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="absolute inset-0 z-20 grid place-items-center bg-black/30 p-8">
      <div className="flex max-h-full w-[560px] flex-col rounded-[10px] border border-border bg-surface-main shadow-floating">
        <div className="border-b border-border-subtle/60 px-5 py-4">
          <div className="text-[16px] font-bold">{tm("memory.manager.edit_summary.title", "Edit Summary Memory")}</div>
          <div className="mt-1 text-xs text-ink-mute">
            {tm(
              "memory.manager.edit_summary.detail",
              "Changes are saved as a new summary version and used for future memory injection.",
            )}
          </div>
        </div>
        <div className="min-h-0 flex-1 p-4">
          <textarea
            className="h-[260px] w-full resize-none rounded-[8px] border border-border-subtle bg-fill/[0.035] p-3 text-[13px] leading-relaxed text-ink outline-none focus:border-brand-blue/50"
            value={draft}
            onChange={(event) => setDraft(event.currentTarget.value)}
          />
        </div>
        <div className="flex justify-end gap-2 border-t border-border-subtle/60 px-4 py-3">
          <Button variant="ghost" disabled={isSaving} onPress={onClose}>
            {tm("common.cancel", "Cancel")}
          </Button>
          <Button variant="primary" disabled={!canSave || isSaving} onPress={() => void save()}>
            {isSaving ? tm("common.processing", "Processing") : tm("common.save", "Save")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function ProjectMemoryMigrationDialog({
  source,
  rows,
  onClose,
  onMigrated,
}: {
  source: MemoryManagerTargetRow;
  rows: MemoryManagerTargetRow[];
  onClose: () => void;
  onMigrated: (projectId: string) => void;
}) {
  const targets = rows.filter(
    (row) => row.scope === "project" && row.isOpenProject && row.projectId && row.projectId !== source.projectId,
  );
  const [targetProjectId, setTargetProjectId] = useState(() => targets[0]?.projectId ?? "");
  const [isSaving, setSaving] = useState(false);
  const target = targets.find((row) => row.projectId === targetProjectId) ?? null;
  const canSubmit = Boolean(source.projectId && targetProjectId && target);

  const submit = async () => {
    if (!source.projectId || !targetProjectId || !target) return;
    const confirmed = await systemConfirm(
      formatI18n(
        tm(
          "memory.manager.migrate_project.confirm.message",
          "Rebind memories from %@ to %@? The source project id will be removed from memory records.",
        ),
        localizedTargetTitle(source),
        localizedTargetTitle(target),
      ),
      {
        title: tm("memory.manager.migrate_project.confirm.title", "Rebind Project Memory"),
        kind: "warning",
        okLabel: tm("memory.manager.migrate_project", "Rebind Project Memory"),
        cancelLabel: tm("common.cancel", "Cancel"),
      },
    );
    if (!confirmed) return;

    let overwrite = false;
    if (target.count > 0) {
      overwrite = await systemConfirm(
        formatI18n(
          tm(
            "memory.manager.migrate_project.overwrite.message",
            "%@ already has memory. Overwrite it before rebinding?",
          ),
          localizedTargetTitle(target),
        ),
        {
          title: tm("memory.manager.migrate_project.overwrite.title", "Overwrite Target Memory"),
          kind: "warning",
          okLabel: tm("common.overwrite", "Overwrite"),
          cancelLabel: tm("common.cancel", "Cancel"),
        },
      );
      if (!overwrite) return;
    }

    setSaving(true);
    try {
      await migrateProjectMemory({
        fromProjectId: source.projectId,
        toProjectId: targetProjectId,
        overwrite,
      });
      onMigrated(targetProjectId);
    } catch (error) {
      await systemMessage(error instanceof Error ? error.message : String(error), {
        title: tm("memory.manager.migrate_project.failed", "Memory migration failed"),
        kind: "error",
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal isOpen onOpenChange={(open) => (!open ? onClose() : undefined)}>
      <Modal.Backdrop className="no-drag fixed inset-0 z-[9000] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
        <Modal.Container size="md" placement="center">
          <Modal.Dialog className="no-drag w-[min(520px,calc(100vw-32px))] rounded-[12px] border border-border bg-surface-main p-4 text-ink shadow-floating outline-none">
            <Modal.Header className="mb-3 p-0">
              <div className="min-w-0">
                <Modal.Heading className="text-sm font-semibold text-ink">
                  {tm("memory.manager.migrate_project.title", "Rebind Project Memory")}
                </Modal.Heading>
                <div className="mt-1 truncate text-xs text-ink-faint">{localizedTargetTitle(source)}</div>
              </div>
            </Modal.Header>
            <div className="grid gap-3">
              <div className="rounded-[8px] bg-fill/[0.04] px-3 py-2">
                <div className="truncate text-[12px] font-semibold text-ink-soft">{localizedTargetTitle(source)}</div>
                <div className="mt-0.5 truncate text-[11px] text-ink-faint">{localizedTargetSubtitle(source)}</div>
              </div>
              {targets.length > 0 ? (
                <label className="grid gap-1.5">
                  <span className="text-sm font-semibold text-ink-soft">
                    {tm("memory.manager.migrate_project.target", "Target Project")}
                  </span>
                  <HeroSelect
                    aria-label={tm("memory.manager.migrate_project.target", "Target Project")}
                    selectedKey={targetProjectId}
                    onSelectionChange={(key) => {
                      if (typeof key === "string") setTargetProjectId(key);
                    }}
                    isDisabled={isSaving}
                    fullWidth
                  >
                    <HeroSelect.Trigger>
                      <HeroSelect.Value />
                      <HeroSelect.Indicator />
                    </HeroSelect.Trigger>
                    <HeroSelect.Popover>
                      <ListBox>
                        {targets.map((row) => (
                          <ListBox.Item
                            key={row.projectId ?? row.id}
                            id={row.projectId ?? row.id}
                            textValue={localizedTargetTitle(row)}
                          >
                            <div className="min-w-0">
                              <div className="truncate text-sm text-ink">{localizedTargetTitle(row)}</div>
                              <div className="truncate text-xs text-ink-faint">{localizedTargetSubtitle(row)}</div>
                            </div>
                            <ListBox.ItemIndicator />
                          </ListBox.Item>
                        ))}
                      </ListBox>
                    </HeroSelect.Popover>
                  </HeroSelect>
                </label>
              ) : (
                <div className="rounded-[8px] border border-border-subtle/70 bg-fill/[0.025] px-3 py-2 text-sm text-ink-mute">
                  {tm("memory.manager.migrate_project.no_targets", "No other open projects are available.")}
                </div>
              )}
            </div>
            <Modal.Footer className="mt-4 flex justify-end gap-2 p-0">
              <Button variant="ghost" size="sm" disabled={isSaving} onPress={onClose}>
                {tm("common.cancel", "Cancel")}
              </Button>
              <Button variant="primary" size="sm" disabled={!canSubmit || isSaving} onPress={() => void submit()}>
                {isSaving
                  ? tm("common.processing", "Processing")
                  : tm("memory.manager.migrate_project", "Rebind Project Memory")}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function IconButton({
  label,
  icon: Icon,
  danger,
  onPress,
}: {
  label: string;
  icon: AppIcon;
  danger?: boolean;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={`grid h-6 w-6 place-items-center rounded-md transition-colors ${danger ? "text-brand-red hover:bg-brand-red/10" : "text-ink-mute hover:bg-fill/[0.08] hover:text-ink"}`}
      onClick={onPress}
    >
      <Icon size={13} strokeWidth={2} />
    </button>
  );
}

function Badge({ text, color }: { text: string; color: string }) {
  return (
    <span
      className="rounded-full px-2 py-0.5 text-[11px] font-semibold"
      style={{ color, backgroundColor: `${color}1c` }}
    >
      {text}
    </span>
  );
}

function EmptyState({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="grid h-full place-items-center px-8 text-center">
      <div>
        <div className="mx-auto grid h-11 w-11 place-items-center rounded-[10px] border border-border-subtle bg-fill/[0.04] text-ink-mute">
          <Zap size={18} />
        </div>
        <div className="mt-3 text-sm font-semibold text-ink">{title}</div>
        <div className="mx-auto mt-1 max-w-[360px] text-xs leading-relaxed text-ink-mute">{detail}</div>
      </div>
    </div>
  );
}

async function indexNow(load: () => Promise<void>, setIndexingNow: (value: boolean) => void) {
  setIndexingNow(true);
  try {
    await flushAppSettings();
    await indexMemoryNow();
    await load();
    const next = useRuntimeStore.getState().memoryManagerSnapshot;
    const error = next?.extraction.status === "failed" ? next.extraction.lastError : null;
    if (error) {
      await systemMessage(error, {
        title: tm("memory.manager.index_failed", "Memory indexing failed"),
        kind: "error",
      });
    }
  } catch (reason) {
    await systemMessage(localizedMemoryIndexError(reason), {
      title: tm("memory.manager.index_failed", "Memory indexing failed"),
      kind: "error",
    });
  } finally {
    setIndexingNow(false);
  }
}

function localizedMemoryIndexError(reason: unknown) {
  const message = reason instanceof Error ? reason.message : String(reason);
  return message.includes("Use For Memory Extraction") || message.includes("AI provider")
    ? tm(
        "memory.status.provider_configuration_needed",
        "Memory needs an enabled AI channel. In Settings > AI, enable a provider and turn on Use For Memory Extraction.",
      )
    : message;
}

async function archiveEntry(entry: MemoryEntry, load: () => Promise<void>) {
  await archiveMemoryEntry(entry.id);
  await load();
}

async function confirmDeleteEntry(entry: MemoryEntry, load: () => Promise<void>) {
  const confirmed = await systemConfirm(
    tm("memory.manager.delete.confirm.message", "This removes the selected memory from the local memory database."),
    {
      title: tm("memory.manager.delete.confirm.title", "Delete Memory"),
      kind: "warning",
      okLabel: tm("common.delete", "Delete"),
      cancelLabel: tm("common.cancel", "Cancel"),
    },
  );
  if (!confirmed) return;
  await deleteMemoryEntry(entry.id);
  await load();
}

async function confirmDeleteSummary(summary: MemorySummary, load: () => Promise<void>) {
  const confirmed = await systemConfirm(
    tm("memory.manager.delete.confirm.message", "This removes the selected memory from the local memory database."),
    {
      title: tm("memory.manager.delete.confirm.title", "Delete Memory"),
      kind: "warning",
      okLabel: tm("common.delete", "Delete"),
      cancelLabel: tm("common.cancel", "Cancel"),
    },
  );
  if (!confirmed) return;
  await deleteMemorySummary(summary.id);
  await load();
}

async function confirmDeleteProjectMemory(projectId: string, load: () => Promise<void>) {
  const confirmed = await systemConfirm(
    tm(
      "memory.manager.delete_project.confirm.message",
      "This removes all project memories and summaries for the selected project from the local memory database.",
    ),
    {
      title: tm("memory.manager.delete_project.confirm.title", "Delete Project Memory"),
      kind: "warning",
      okLabel: tm("common.delete", "Delete"),
      cancelLabel: tm("common.cancel", "Cancel"),
    },
  );
  if (!confirmed) return;
  await deleteProjectMemory(projectId);
  await load();
}

function fallbackTargets(): MemoryManagerTargetRow[] {
  return [
    {
      id: "user",
      scope: "user",
      projectId: null,
      title: tm("memory.manager.user_memory", "User Memory"),
      subtitle: tm("memory.manager.user_memory.subtitle", "Cross-project preferences"),
      count: 0,
      updatedAt: null,
      isOpenProject: false,
    },
  ];
}

function localizedTargetTitle(row: MemoryManagerTargetRow) {
  return row.scope === "user" ? tm("memory.manager.user_memory", "User Memory") : row.title;
}

function localizedTargetSubtitle(row: MemoryManagerTargetRow) {
  return row.scope === "user" ? tm("memory.manager.user_memory.subtitle", "Cross-project preferences") : row.subtitle;
}

function kindTitle(kind: MemoryKind) {
  return tm(`memory.kind.${kind}`, kind);
}

function tierTitle(tier: string) {
  return tm(`memory.tier.${tier}`, tier);
}

function statusTitle(status: string) {
  return tm(`memory.status.${status}`, status);
}

function formatDate(seconds: number) {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
