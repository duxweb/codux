import { Folder, ListChecks, Plus, RefreshCw, SquareTerminal } from "../icons";
import { Input as HeroInput, ListBox, Modal, Select as HeroSelect } from "@heroui/react";
import { memo, useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { useAIHistorySnapshot } from "../ai/history";
import { formatI18n, tm } from "../i18n";
import { revealProjectInFileManager } from "../ide";
import { Button } from "./Button";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import { Checkbox } from "./Form";
import { PressableButton } from "./PressableButton";
import { normalizeGitEventPath } from "../git/status";
import { useRuntimeStore } from "../runtimeStore";
import type { WorkspaceProject } from "../types";
import { worktreeBranchOptions } from "../worktree/branches";
import { readAppSettings, subscribeAppSettings, type AIStatisticsMode } from "../settings";
import { AIHistorySessionList } from "./AIHistorySessionList";
import type { ProjectWorktreeSnapshot, WorktreeTaskStatus } from "../worktree/snapshot";

type WorktreeAIState = WorkspaceProject["aiState"];

type WorktreeRow = {
  id: string;
  title: string;
  branch: string;
  changes?: number;
  outgoing?: number;
  incoming?: number;
  additions?: number;
  deletions?: number;
  status: WorktreeTaskStatus;
  worktree: ProjectWorktreeSnapshot;
};

type Props = {
  selectedProject?: WorkspaceProject;
  worktrees?: ProjectWorktreeSnapshot[];
  selectedWorktreeId?: string;
  aiStateByWorktreeId?: Record<string, WorktreeAIState>;
  onSelectWorktree?: (id: string) => void;
  onCreateWorktree?: (input: { branchName: string; baseBranch?: string | null }) => void;
  onRemoveWorktree?: (worktree: ProjectWorktreeSnapshot, options?: { removeBranch?: boolean }) => void;
  onMergeWorktree?: (worktree: ProjectWorktreeSnapshot, options?: { removeBranch?: boolean }) => void;
  onOpenWorktreeTerminal?: (worktree: ProjectWorktreeSnapshot) => void;
  onReviewWorktree?: (worktree: ProjectWorktreeSnapshot) => void;
  onRefreshWorktrees?: () => void;
  isBusy?: boolean;
  createRequest?: number;
  canCreateWorktree?: boolean;
  repositoryMessage?: string;
};

export function TaskSidebar({
  selectedProject,
  worktrees = [],
  selectedWorktreeId,
  aiStateByWorktreeId = {},
  onSelectWorktree,
  onCreateWorktree,
  onRemoveWorktree,
  onMergeWorktree,
  onOpenWorktreeTerminal,
  onReviewWorktree,
  onRefreshWorktrees,
  isBusy,
  createRequest = 0,
  canCreateWorktree = true,
  repositoryMessage = "",
}: Props) {
  const [isCreating, setCreating] = useState(false);
  const [worktreeName, setWorktreeName] = useState("");
  const [baseBranch, setBaseBranch] = useState("");
  const [createError, setCreateError] = useState("");
  const [optimisticSelectedId, setOptimisticSelectedId] = useState("");
  const [historyHeight, setHistoryHeight] = useState<number | null>(null);
  const [pendingRemoveWorktree, setPendingRemoveWorktree] = useState<WorktreeRow | null>(null);
  const [removeBranchWithWorktree, setRemoveBranchWithWorktree] = useState(false);
  const [statisticsMode, setStatisticsMode] = useState<AIStatisticsMode>(
    () => readAppSettings().statisticsMode as AIStatisticsMode,
  );
  const asideRef = useRef<HTMLElement | null>(null);
  const defaultWorktree = worktrees.find((worktree) => worktree.isDefault) ?? fallbackDefaultWorktree(selectedProject);
  const gitSnapshot = useRuntimeStore((state) =>
    selectedProject?.path ? state.gitStatusByPath[normalizeGitEventPath(selectedProject.path)]?.snapshot : undefined,
  );
  const worktreeRows = useMemo(
    () =>
      [defaultWorktree, ...worktrees.filter((worktree) => worktree.id !== defaultWorktree.id)].map((worktree) =>
        toWorktreeRow(worktree),
      ),
    [defaultWorktree, worktrees],
  );
  const branchOptions = useMemo(
    () =>
      worktreeBranchOptions([
        gitSnapshot?.branch ?? "",
        ...(gitSnapshot?.branches.map((branch) => branch.name) ?? []),
        defaultWorktree.branch ?? "",
        selectedProject?.branch ?? "",
      ]),
    [defaultWorktree.branch, gitSnapshot, selectedProject?.branch],
  );
  const canCreate = canCreateWorktree && worktreeName.trim().length > 0 && baseBranch.trim().length > 0 && !isBusy;
  const optimisticRowExists = optimisticSelectedId
    ? worktreeRows.some((worktree) => worktree.id === optimisticSelectedId)
    : false;
  const selectedRowId = (optimisticRowExists ? optimisticSelectedId : "") || selectedWorktreeId || worktreeRows[0]?.id;
  const selectedWorktree = worktreeRows.find((worktree) => worktree.id === selectedRowId)?.worktree;
  const historyProject = useMemo<WorkspaceProject | undefined>(() => {
    if (!selectedProject) return undefined;
    if (!selectedWorktree) return selectedProject;
    return {
      ...selectedProject,
      id: selectedWorktree.id,
      rootProjectId: selectedProject.id,
      worktreeId: selectedWorktree.id,
      name: selectedWorktree.isDefault ? selectedProject.name : `${selectedProject.name} · ${selectedWorktree.name}`,
      path: selectedWorktree.path || selectedProject.path,
      branch: selectedWorktree.branch || selectedProject.branch,
      baseBranch: selectedWorktree.isDefault ? null : selectedProject.branch,
      isDefaultWorktree: selectedWorktree.isDefault,
      changes: selectedWorktree.gitSummary.changes,
    };
  }, [selectedProject, selectedWorktree]);
  const history = useAIHistorySnapshot(historyProject, { includeSessions: true });

  const selectWorktree = useCallback(
    (id: string) => {
      if (selectedRowId !== id) {
        setOptimisticSelectedId(id);
      }
      onSelectWorktree?.(id);
    },
    [onSelectWorktree, selectedRowId],
  );

  useEffect(() => {
    setOptimisticSelectedId(selectedWorktreeId ?? "");
  }, [selectedWorktreeId, selectedProject?.id]);

  useEffect(
    () =>
      subscribeAppSettings((settings) => {
        setStatisticsMode(settings.statisticsMode as AIStatisticsMode);
      }),
    [],
  );

  const openCreateModal = useCallback(() => {
    if (!canCreateWorktree) return;
    setCreating(true);
    setWorktreeName(timestampSlug());
    setBaseBranch(branchOptions[0] ?? "");
    setCreateError("");
  }, [branchOptions, canCreateWorktree]);

  useEffect(() => {
    if (createRequest <= 0) return;
    openCreateModal();
  }, [createRequest, openCreateModal]);

  const submitCreate = () => {
    const nextName = worktreeName.trim();
    const nextBaseBranch = baseBranch.trim();
    if (!nextName || !nextBaseBranch) return;
    setCreateError("");
    Promise.resolve(
      onCreateWorktree?.({
        branchName: nextName,
        baseBranch: nextBaseBranch,
      }),
    )
      .then(() => {
        setCreating(false);
        setWorktreeName("");
      })
      .catch((error) => {
        setCreateError(error instanceof Error ? error.message : String(error));
      });
  };

  const handleCreatePress = () => {
    if (canCreate) submitCreate();
  };

  const openRemoveConfirm = (worktree: WorktreeRow) => {
    setPendingRemoveWorktree(worktree);
    setRemoveBranchWithWorktree(false);
  };

  const closeRemoveConfirm = () => {
    setPendingRemoveWorktree(null);
    setRemoveBranchWithWorktree(false);
  };

  const confirmRemoveWorktree = () => {
    if (!pendingRemoveWorktree) return;
    onRemoveWorktree?.(pendingRemoveWorktree.worktree, { removeBranch: removeBranchWithWorktree });
    closeRemoveConfirm();
  };

  const beginHistoryResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startY = event.clientY;
    const asideHeight = asideRef.current?.clientHeight ?? 640;
    const startHeight = historyHeight ?? Math.round(asideHeight * 0.4);
    const maxHeight = Math.max(180, Math.round(asideHeight * 0.58));
    const handlePointerMove = (moveEvent: PointerEvent) => {
      const nextHeight = startHeight - (moveEvent.clientY - startY);
      setHistoryHeight(Math.max(156, Math.min(maxHeight, Math.round(nextHeight))));
    };
    const handlePointerUp = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerUp);
    };
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerUp);
  };

  return (
    <aside ref={asideRef} className="h-full flex flex-col">
      <div className="h-[42px] px-3.5 flex items-center justify-between flex-shrink-0">
        <span className="min-w-0 truncate text-sm font-semibold tracking-tight">
          {selectedProject?.name || tm("worktree.sidebar.title", "Worktree")}
        </span>
        <div className="flex items-center gap-1">
          <PressableButton
            className="w-6 h-6 grid place-items-center rounded-md text-ink-mute hover:text-ink hover:bg-fill/8 transition-colors disabled:opacity-50"
            disabled={isBusy || !onRefreshWorktrees}
            aria-label={tm("common.refresh", "Refresh")}
            onPressUp={onRefreshWorktrees}
          >
            <RefreshCw size={12} strokeWidth={2.4} className={isBusy ? "animate-spin" : ""} />
          </PressableButton>
          {canCreateWorktree && (
            <PressableButton
              className="w-6 h-6 grid place-items-center rounded-md text-ink-mute hover:text-ink hover:bg-fill/8 transition-colors disabled:opacity-50"
              disabled={isBusy}
              aria-label={tm("worktree.create.title", "New Worktree")}
              onPressUp={openCreateModal}
            >
              <Plus size={12} strokeWidth={2.4} />
            </PressableButton>
          )}
        </div>
      </div>
      <div className="h-px bg-border-subtle opacity-60" />

      <div className="min-h-0 flex-1 overflow-y-auto scrollbar-overlay px-2 pt-3 pb-2.5">
        {worktreeRows.length > 0 ? (
          worktreeRows.map((worktree) => (
            <WorktreeCard
              key={worktree.id}
              worktree={worktree}
              aiState={aiStateByWorktreeId[worktree.id] ?? "idle"}
              isSelected={selectedRowId === worktree.id}
              onSelect={() => selectWorktree(worktree.id)}
              onRemove={worktree.worktree.isDefault ? undefined : () => openRemoveConfirm(worktree)}
              onMerge={worktree.worktree.isDefault ? undefined : (options) => onMergeWorktree?.(worktree.worktree, options)}
              onOpenTerminal={() => {
                onOpenWorktreeTerminal?.(worktree.worktree);
              }}
              onOpenFolder={() => {
                if (worktree.worktree.path) void revealProjectInFileManager(worktree.worktree.path);
              }}
              onReview={() => {
                onSelectWorktree?.(worktree.id);
                onReviewWorktree?.(worktree.worktree);
              }}
              repositoryMessage={repositoryMessage}
            />
          ))
        ) : (
          <div className="px-2 py-2 text-xs leading-relaxed text-ink-faint">
            {tm("worktree.sidebar.empty", "No worktrees")}
          </div>
        )}
      </div>
      <div
        className="relative flex-none bg-transparent"
        style={{ height: historyHeight ?? "40%" }}
      >
        <div
          className="peer/history-resize absolute inset-x-0 top-[-4px] z-10 h-3 cursor-row-resize"
          onPointerDown={beginHistoryResize}
          aria-label={tm("common.resize", "Resize")}
        />
        <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-border-subtle transition-colors peer-hover/history-resize:bg-border" />
        <div className="flex h-[38px] items-center justify-between gap-2 bg-fill/[0.045] px-3.5 py-1.5">
          <span className="truncate text-sm font-semibold text-ink-soft">
            {tm("ai.sessions.history", "Session History")}
          </span>
          {history.snapshot.sessions.length > 0 && (
            <span className="flex h-5 min-w-5 flex-none items-center justify-center rounded-full bg-fill/[0.08] px-1.5 text-[11px] font-semibold tabular-nums text-ink-faint">
              {history.snapshot.sessions.length}
            </span>
          )}
        </div>
        <AIHistorySessionList
          project={historyProject}
          sessions={history.snapshot.sessions}
          mode={statisticsMode}
          isLoading={history.isLoading}
          error={history.error}
          maxItems={12}
          className="h-[calc(100%-38px)] px-2 pt-2.5 pb-2.5"
        />
      </div>
      <Modal isOpen={isCreating} onOpenChange={setCreating}>
        <Modal.Backdrop className="no-drag fixed inset-0 z-[9000] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
          <Modal.Container size="sm" placement="center">
            <Modal.Dialog className="no-drag w-[min(380px,calc(100vw-32px))] rounded-[12px] border border-border bg-surface-main p-4 text-ink shadow-floating outline-none">
              <Modal.Header className="mb-3 p-0">
                <div className="min-w-0">
                  <Modal.Heading className="text-sm font-semibold text-ink">
                    {tm("worktree.create.title", "New Worktree")}
                  </Modal.Heading>
                  <div className="mt-1 truncate text-xs text-ink-faint">
                    {selectedProject?.name ?? selectedProject?.path ?? ""}
                  </div>
                </div>
              </Modal.Header>
              <form
                className="grid gap-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  if (canCreate) submitCreate();
                }}
              >
                <label className="grid gap-1.5">
                  <span className="text-sm font-semibold text-ink-soft">
                    {tm("worktree.task.base_branch", "Base Branch")}
                  </span>
                  <HeroSelect
                    aria-label={tm("worktree.task.base_branch", "Base Branch")}
                    selectedKey={baseBranch}
                    onSelectionChange={(key) => {
                      if (typeof key === "string") setBaseBranch(key);
                    }}
                    isDisabled={isBusy || branchOptions.length === 0}
                    fullWidth
                  >
                    <HeroSelect.Trigger>
                      <HeroSelect.Value />
                      <HeroSelect.Indicator />
                    </HeroSelect.Trigger>
                    <HeroSelect.Popover>
                      <ListBox>
                        {branchOptions.map((branch) => (
                          <ListBox.Item key={branch} id={branch} textValue={branch}>
                            {branch}
                            <ListBox.ItemIndicator />
                          </ListBox.Item>
                        ))}
                      </ListBox>
                    </HeroSelect.Popover>
                  </HeroSelect>
                </label>
                <label className="grid gap-1.5">
                  <span className="text-sm font-semibold text-ink-soft">
                    {tm("worktree.task.title", "Worktree Name")}
                  </span>
                  <HeroInput
                    value={worktreeName}
                    onChange={(event) => setWorktreeName(event.currentTarget.value)}
                    disabled={isBusy}
                    fullWidth
                    autoFocus
                  />
                </label>
                {createError ? <div className="text-sm text-brand-red">{createError}</div> : null}
                <Modal.Footer className="mt-1 flex justify-end gap-2 p-0">
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={isBusy}
                    onPressUp={() => {
                      setCreating(false);
                    }}
                  >
                    {tm("common.cancel", "Cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={!canCreate}
                    className="bg-brand-blue text-on-brand"
                    onPressUp={handleCreatePress}
                  >
                    {isBusy ? tm("common.creating", "Creating") : tm("common.create", "Create")}
                  </Button>
                </Modal.Footer>
              </form>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
      <Modal isOpen={Boolean(pendingRemoveWorktree)} onOpenChange={(open) => (!open ? closeRemoveConfirm() : undefined)}>
        <Modal.Backdrop className="no-drag fixed inset-0 z-[9000] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
          <Modal.Container size="sm" placement="center">
            <Modal.Dialog className="no-drag w-[min(400px,calc(100vw-32px))] rounded-[12px] border border-border bg-surface-main p-4 text-ink shadow-floating outline-none">
              <Modal.Header className="mb-3 p-0">
                <div className="min-w-0">
                  <Modal.Heading className="text-sm font-semibold text-ink">
                    {tm("worktree.remove.title", "Remove Worktree")}
                  </Modal.Heading>
                  <div className="mt-1 truncate text-xs text-ink-faint">
                    {pendingRemoveWorktree?.title ?? ""}
                  </div>
                </div>
              </Modal.Header>
              <div className="grid gap-3">
                <p className="text-sm leading-5 text-ink-soft">
                  {formatI18n(
                    tm("worktree.remove.message_format", "Remove %@ from Codux and the Git worktree list? The branch will not be deleted."),
                    pendingRemoveWorktree?.branch || pendingRemoveWorktree?.title || "",
                  )}
                </p>
                <div className="rounded-[8px] bg-fill/[0.04] px-3 py-2">
                  <Checkbox
                    checked={removeBranchWithWorktree}
                    onChange={setRemoveBranchWithWorktree}
                    label={tm("worktree.remove.delete_branch_checkbox", "Also delete the local branch")}
                  />
                </div>
                {removeBranchWithWorktree && (
                  <p className="text-xs leading-5 text-brand-red">
                    {tm("worktree.remove.delete_branch_warning", "Deleting the branch cannot be undone.")}
                  </p>
                )}
                <Modal.Footer className="mt-1 flex justify-end gap-2 p-0">
                  <Button size="sm" variant="ghost" onPressUp={closeRemoveConfirm}>
                    {tm("common.cancel", "Cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="primary"
                    className={removeBranchWithWorktree ? "bg-brand-red text-on-brand" : "bg-brand-blue text-on-brand"}
                    onPressUp={confirmRemoveWorktree}
                  >
                    {removeBranchWithWorktree
                      ? tm("worktree.menu.remove_with_branch", "Remove and Delete Branch")
                      : tm("worktree.menu.remove", "Remove")}
                  </Button>
                </Modal.Footer>
              </div>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </aside>
  );
}

const WorktreeCard = memo(function WorktreeCard({
  worktree,
  aiState,
  isSelected,
  onSelect,
  onRemove,
  onMerge,
  onOpenTerminal,
  onOpenFolder,
  onReview,
  repositoryMessage,
}: {
  worktree: WorktreeRow;
  aiState: WorktreeAIState;
  isSelected?: boolean;
  onSelect?: () => void;
  onRemove?: (options?: { removeBranch?: boolean }) => void;
  onMerge?: (options?: { removeBranch?: boolean }) => void;
  onOpenTerminal?: () => void;
  onOpenFolder?: () => void;
  onReview?: () => void;
  repositoryMessage?: string;
}) {
  const contextMenu = useContextMenu();
  const interactionBg = isSelected ? "bg-brand-blue/14" : "hover:bg-fill/4";
  const menuItems = (
    <>
      <ContextMenuItem label={tm("worktree.menu.open_terminal", "Open Terminal")} onSelect={onOpenTerminal}>
        <SquareTerminal size={13} />
        {tm("worktree.menu.open_terminal", "Open Terminal")}
      </ContextMenuItem>
      <ContextMenuItem
        label={tm("worktree.menu.open_folder", "Open Folder")}
        onSelect={onOpenFolder}
        disabled={!worktree.worktree.path}
      >
        <Folder size={13} />
        {tm("worktree.menu.open_folder", "Open Folder")}
      </ContextMenuItem>
      <ContextMenuItem label={tm("worktree.menu.review", "Review")} onSelect={onReview}>
        <ListChecks size={13} />
        {tm("worktree.menu.review", "Review")}
      </ContextMenuItem>
      {onRemove && (
        <>
          <ContextMenuSeparator />
          {onMerge && (
            <>
              <ContextMenuItem label={tm("worktree.menu.merge", "Merge to Mainline")} onSelect={() => onMerge()}>
                {tm("worktree.menu.merge", "Merge to Mainline")}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          <ContextMenuItem label={tm("worktree.menu.remove", "Remove")} onSelect={onRemove}>
            {tm("worktree.menu.remove", "Remove")}
          </ContextMenuItem>
        </>
      )}
    </>
  );

  return (
    <div className="group relative mb-1.5 contain-layout" onContextMenu={contextMenu.openMenu}>
      <PressableButton
        onPressUp={onSelect}
        className="relative w-full min-h-[64px] rounded-[8px] overflow-hidden text-left"
      >
        <span className={`absolute inset-0 rounded-[8px] ${interactionBg}`} />

        <div className="relative flex min-h-[64px] items-center gap-2.5 px-2.5 py-2.5">
          <span className="grid h-full w-4 flex-shrink-0 place-items-center">
            <WorktreeActivityDot state={aiState} />
          </span>
          <div className="min-w-0 flex-1">
            <div
              className={`break-all text-sm font-semibold leading-snug ${isSelected ? "text-ink" : "text-ink-soft"}`}
            >
              {worktree.title}
            </div>
            <WorktreeGitSummary
              changes={worktree.changes ?? 0}
              additions={worktree.additions ?? 0}
              deletions={worktree.deletions ?? 0}
              repositoryMessage={worktree.worktree.isDefault ? repositoryMessage : ""}
            />
          </div>
        </div>
      </PressableButton>
      <ContextMenu
        ariaLabel={formatI18n(tm("worktree.menu.actions_format", "%@ Actions"), worktree.title)}
        menu={contextMenu.menu}
        onClose={contextMenu.closeMenu}
      >
        {menuItems}
      </ContextMenu>
    </div>
  );
});

function WorktreeActivityDot({ state }: { state: WorktreeAIState }) {
  if (state === "running") {
    return (
      <span className="relative grid h-3 w-3 place-items-center rounded-full" aria-hidden="true">
        <span className="absolute h-3 w-3 rounded-full bg-brand-amber/70 motion-safe:animate-ping" />
        <span className="relative h-2.5 w-2.5 rounded-full bg-brand-amber" />
      </span>
    );
  }
  if (state === "review") {
    return <span className="h-2.5 w-2.5 rounded-full bg-brand-amber" />;
  }
  if (state === "done") {
    return <span className="h-2.5 w-2.5 rounded-full bg-brand-green" />;
  }
  return <span className="h-2.5 w-2.5 rounded-full bg-brand-blue" />;
}

function WorktreeGitSummary({
  changes,
  additions,
  deletions,
  repositoryMessage,
}: {
  changes: number;
  additions: number;
  deletions: number;
  repositoryMessage?: string;
}) {
  if (repositoryMessage) {
    return <div className="mt-1 truncate text-xs font-semibold text-ink-faint">{repositoryMessage}</div>;
  }
  return (
    <div className="mt-1 flex min-w-0 items-center justify-between gap-2 text-xs font-semibold tabular-nums">
      <span className="min-w-0 truncate text-ink-faint">
        {formatI18n(tm("worktree.sidebar.changed_format", "%@ changed"), changes)}
      </span>
      <span className="flex flex-none items-center gap-1.5">
        <span className="text-brand-green">+{Math.max(0, additions)}</span>
        <span className="text-brand-red">-{Math.max(0, deletions)}</span>
      </span>
    </div>
  );
}

function fallbackDefaultWorktree(project?: WorkspaceProject): ProjectWorktreeSnapshot {
  const branch = project?.branch?.trim() || "main";
  return {
    id: project?.id ?? "main",
    projectId: project?.id ?? "",
    name: branch,
    branch,
    path: project?.path ?? "",
    status: "todo",
    isDefault: true,
    createdAt: 0,
    updatedAt: 0,
    gitSummary: {
      changes: project?.changes ?? 0,
      incoming: 0,
      outgoing: 0,
      additions: 0,
      deletions: 0,
    },
  };
}

function toWorktreeRow(worktree: ProjectWorktreeSnapshot): WorktreeRow {
  const status = worktree.status;
  const branch = worktree.branch?.trim() || "main";
  return {
    id: worktree.id,
    title: worktree.isDefault ? branch : worktree.name || branchTitle(branch) || branch,
    branch: branch || worktree.path,
    changes: worktree.gitSummary.changes,
    incoming: worktree.gitSummary.incoming,
    outgoing: worktree.gitSummary.outgoing,
    additions: worktree.gitSummary.additions,
    deletions: worktree.gitSummary.deletions,
    status,
    worktree,
  };
}

function timestampSlug() {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function branchTitle(branch: string) {
  return branch.split("/").filter(Boolean).pop() || "";
}
