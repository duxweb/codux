import { invoke } from "@tauri-apps/api/core";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { AIHistorySessionSummary } from "../ai/history";
import type { AIStatisticsMode } from "../settings";
import type { WorkspaceProject } from "../types";
import { formatI18n, localeFromSettings, tm } from "../i18n";
import { dispatchWorkspaceCommand } from "../workspaceCommands";
import { RefreshCw } from "../icons";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import { PressableButton } from "./PressableButton";
import { systemConfirm } from "../systemDialog";

type Props = {
  project?: WorkspaceProject;
  sessions: AIHistorySessionSummary[];
  mode: AIStatisticsMode;
  isLoading?: boolean;
  error?: string | null;
  className?: string;
  maxItems?: number;
};

const relativeTimeFormatters = new Map<string, Intl.RelativeTimeFormat>();
const restorePendingTimeoutMs = 6_000;

export function AIHistorySessionList({ project, sessions, mode, isLoading, error, className, maxItems = 20 }: Props) {
  const [selectedSessionId, setSelectedSessionId] = useState("");
  const [restoringSessionId, setRestoringSessionId] = useState("");
  const [titleOverrides, setTitleOverrides] = useState<Record<string, string>>({});
  const [hiddenSessionIds, setHiddenSessionIds] = useState<Set<string>>(() => new Set());
  const restoreTimeoutRef = useRef<number | null>(null);
  const restoreLockRef = useRef<string | null>(null);
  const rows = useMemo(
    () =>
      sessions
        .filter((session) => !hiddenSessionIds.has(session.sessionId))
        .slice(0, maxItems)
        .map((session) => ({
          ...session,
          sessionTitle: titleOverrides[session.sessionId] ?? session.sessionTitle,
        })),
    [hiddenSessionIds, maxItems, sessions, titleOverrides],
  );

  useEffect(
    () => () => {
      if (restoreTimeoutRef.current != null) {
        window.clearTimeout(restoreTimeoutRef.current);
      }
    },
    [],
  );

  const renameSession = (session: AIHistorySessionSummary) => {
    const nextTitle = window.prompt(tm("common.rename", "Rename"), session.sessionTitle)?.trim();
    if (!nextTitle) return;
    if (!project || !window.__TAURI_INTERNALS__) {
      setTitleOverrides((current) => ({
        ...current,
        [session.sessionId]: nextTitle,
      }));
      return;
    }
    void invoke("ai_history_session_rename", {
      project: {
        id: project.id,
        name: project.name,
        path: project.path,
      },
      sessionId: session.sessionId,
      title: nextTitle,
    }).catch((reason) => console.error("failed to rename ai history session", reason));
  };

  const deleteSession = (session: AIHistorySessionSummary) => {
    void systemConfirm(formatI18n(tm("ai.sessions.delete_confirm_format", "Delete %@?"), session.sessionTitle), {
      title: tm("common.delete", "Delete"),
      kind: "warning",
      okLabel: tm("common.delete", "Delete"),
      cancelLabel: tm("common.cancel", "Cancel"),
    }).then((confirmed) => {
      if (!confirmed) return;
      if (!project || !window.__TAURI_INTERNALS__) {
        setHiddenSessionIds((current) => new Set(current).add(session.sessionId));
        setSelectedSessionId((current) => (current === session.sessionId ? "" : current));
        return;
      }
      void invoke("ai_history_session_remove", {
        project: {
          id: project.id,
          name: project.name,
          path: project.path,
        },
        sessionId: session.sessionId,
      }).catch((reason) => console.error("failed to remove ai history session", reason));
    });
  };

  const restoreSession = (session: AIHistorySessionSummary) => {
    if (!project || restoreLockRef.current) return;
    restoreLockRef.current = session.sessionId;
    setRestoringSessionId(session.sessionId);
    if (restoreTimeoutRef.current != null) {
      window.clearTimeout(restoreTimeoutRef.current);
    }
    restoreTimeoutRef.current = window.setTimeout(() => {
      restoreTimeoutRef.current = null;
      restoreLockRef.current = null;
      setRestoringSessionId((current) => (current === session.sessionId ? "" : current));
    }, restorePendingTimeoutMs);
    restoreHistorySession(project, session);
  };

  return (
    <div className={`min-h-0 overflow-y-auto scrollbar-overlay ${className ?? ""}`}>
      {rows.map((session) => (
        <HistorySessionRow
          key={session.sessionId}
          session={session}
          mode={mode}
          selected={selectedSessionId === session.sessionId}
          restoring={restoringSessionId === session.sessionId}
          disabled={Boolean(restoringSessionId)}
          onSelect={() => setSelectedSessionId(session.sessionId)}
          onRestore={() => restoreSession(session)}
          onRename={() => renameSession(session)}
          onDelete={() => deleteSession(session)}
        />
      ))}
      {rows.length === 0 && (
        <div className="px-2.5 py-3 text-xs text-ink-faint">
          {isLoading
            ? tm("ai.indexing.reading_sources", "Reading index.")
            : error
              ? tm("ai.session.storage.open_failed", "Unable to open session storage.")
              : tm("ai.sessions.empty", "No Session History")}
        </div>
      )}
    </div>
  );
}

const HistorySessionRow = memo(function HistorySessionRow({
  session,
  mode,
  selected,
  restoring,
  disabled,
  onSelect,
  onRestore,
  onRename,
  onDelete,
}: {
  session: AIHistorySessionSummary;
  mode: AIStatisticsMode;
  selected?: boolean;
  restoring?: boolean;
  disabled?: boolean;
  onSelect: () => void;
  onRestore: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const contextMenu = useContextMenu();
  const tool = session.lastTool || "-";
  const lastSeenLabel = sessionTimeLabel(session.lastSeenAt);
  const totalLabel = formatTokens(displayedHistorySessionTotal(session, mode));

  return (
    <div className="relative mb-1 last:mb-0">
      <PressableButton
        className={`w-full rounded-[8px] px-2.5 py-2 text-left outline-none transition-colors ${
          selected ? "bg-brand-blue/13" : disabled ? "opacity-65" : "hover:bg-fill/[0.055]"
        }`}
        disabled={disabled}
        aria-busy={restoring}
        onPressUp={onSelect}
        onDoubleClick={onRestore}
        onContextMenu={(event) => {
          onSelect();
          contextMenu.openMenu(event);
        }}
      >
        <div className="flex min-w-0 items-start justify-between gap-2">
          <div className="min-w-0 flex-1 truncate text-sm font-semibold leading-5 text-ink">{session.sessionTitle}</div>
          <div className="flex-none whitespace-nowrap text-[11.5px] font-medium leading-5 text-ink-faint">
            {lastSeenLabel}
          </div>
        </div>
        <div className="mt-1.5 flex min-w-0 items-center justify-between gap-2 text-[11.5px] font-medium leading-4 text-ink-faint">
          <div className="min-w-0 truncate text-ink-faint">
            {restoring ? (
              <span className="inline-flex min-w-0 items-center gap-1.5">
                <RefreshCw className="h-3 w-3 shrink-0 animate-spin" />
                <span className="truncate">{tm("common.creating", "Creating")}</span>
              </span>
            ) : (
              tool
            )}
          </div>
          <div className="flex-none text-xs font-medium tabular-nums leading-4 text-ink-mute">{totalLabel}</div>
        </div>
      </PressableButton>
      <ContextMenu
        ariaLabel={formatI18n(tm("ai.sessions.actions_format", "%@ Actions"), session.sessionTitle)}
        menu={contextMenu.menu}
        onClose={contextMenu.closeMenu}
      >
        <ContextMenuItem label={tm("common.open", "Open")} onSelect={onRestore}>
          {tm("common.open", "Open")}
        </ContextMenuItem>
        <ContextMenuItem label={tm("common.rename", "Rename")} onSelect={onRename}>
          {tm("common.rename", "Rename")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem label={tm("common.delete", "Delete")} onSelect={onDelete}>
          {tm("common.delete", "Delete")}
        </ContextMenuItem>
      </ContextMenu>
    </div>
  );
});

function restoreHistorySession(project: WorkspaceProject | undefined, session: AIHistorySessionSummary) {
  if (!project) return;
  dispatchWorkspaceCommand({
    type: "add-top-terminal-split",
    projectId: project.id,
    projectPath: project.path,
    projectName: project.name,
    title: session.sessionTitle,
    deferredCommand: historySessionRestoreCommand(session),
  });
}

function historySessionRestoreCommand(session: AIHistorySessionSummary) {
  const tool = (session.lastTool || "").toLowerCase();
  const id = session.externalSessionId || session.sessionId;
  const quotedId = shellQuote(id);
  if (tool.includes("codex")) return `codex resume ${quotedId}`;
  if (tool.includes("claude")) return `claude --resume ${quotedId}`;
  if (tool.includes("agy") || tool.includes("antigravity")) return `agy resume ${quotedId}`;
  if (tool.includes("gemini")) return `gemini resume ${quotedId}`;
  if (tool.includes("opencode")) return `opencode run --session ${quotedId}`;
  return `codex resume ${quotedId}`;
}

function displayedHistorySessionTotal(session: AIHistorySessionSummary, mode: AIStatisticsMode) {
  return session.totalTokens + (mode === "includingCache" ? session.cachedInputTokens : 0);
}

function formatTokens(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(Math.max(0, Math.floor(value)));
}

function sessionTimeLabel(timestamp: number) {
  if (!Number.isFinite(timestamp) || timestamp <= 0) return "-";
  return relativeSessionTime(new Date(timestamp * 1000));
}

function relativeSessionTime(date: Date) {
  const formatter = cachedRelativeTimeFormatter(localeFromSettings());
  const diffMs = Math.min(0, date.getTime() - Date.now());
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (Math.abs(diffMs) < minute) return tm("common.just_now", "刚刚");
  if (Math.abs(diffMs) < hour) return formatter.format(Math.round(diffMs / minute), "minute");
  if (Math.abs(diffMs) < day) return formatter.format(Math.round(diffMs / hour), "hour");
  return formatter.format(Math.round(diffMs / day), "day");
}

function cachedRelativeTimeFormatter(locale: string) {
  let formatter = relativeTimeFormatters.get(locale);
  if (!formatter) {
    formatter = new Intl.RelativeTimeFormat(locale, {
      numeric: "always",
      style: "short",
    });
    relativeTimeFormatters.set(locale, formatter);
  }
  return formatter;
}

function shellQuote(value: string) {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
