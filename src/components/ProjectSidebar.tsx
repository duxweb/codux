import {
  ArrowTopRight,
  Box,
  Bug,
  Download,
  FileText,
  Folder,
  Info,
  MoreHorizontal,
  Plus,
  Server,
  Settings,
  TerminalSquare,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import {
  checkForUpdates,
  closeProjectFromMenu,
  exportDiagnostics,
  openExternalUrl,
  openLiveLog,
  openRuntimeLog,
  showAbout,
  toggleDeveloperTools,
} from "../appActions";
import { CODUX_GITHUB_URL, CODUX_WEBSITE_URL } from "../appLinks";
import { openAppWindow } from "../windowing";
import { useCallback, useEffect, useState } from "react";
import { revealProjectInFileManager } from "../ide";
import { formatI18n, t, tm } from "../i18n";
import { Button } from "./Button";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import { DesktopMenu, DesktopMenuItem, DesktopMenuSeparator } from "./DesktopMenu";
import { PressableButton } from "./PressableButton";
import { Tooltip } from "./Tooltip";
import type { WorkspaceProject } from "../types";
import type { AppIcon } from "../icons";

type Props = {
  projects: WorkspaceProject[];
  selectedProjectId: string;
  onSelect: (id: string) => void;
  isExpanded: boolean;
  onFocusScope: () => void;
  onCreateProject: () => void;
  onOpenSettings: () => void;
  onCreateWorktree?: (project: WorkspaceProject) => void;
};

const badgePalette = [
  { from: "#5b8df4", to: "#3a6fd9" },
  { from: "#b495ea", to: "#7e58c4" },
  { from: "#e25b8b", to: "#b53d6f" },
  { from: "#39d98a", to: "#1f9d61" },
  { from: "#f4b85a", to: "#c98933" },
];

function projectBadgeColors(color: string) {
  return {
    from: color,
    to: `color-mix(in oklab, ${color} 72%, black)`,
  };
}

const projectBadgeIcons: Record<string, AppIcon> = {
  terminal: TerminalSquare,
  folder: Folder,
  shippingbox: Box,
  "shippingbox.fill": Box,
  hammer: Bug,
  "server.rack": Server,
  globe: ArrowTopRight,
  bolt: Download,
  wrench: Settings,
  "doc.text": FileText,
  laptopcomputer: Settings,
  "cube.box": Box,
  paintpalette: Settings,
  sparkles: Bug,
  book: Info,
  "person.2": Info,
};

export function ProjectSidebar({
  projects,
  selectedProjectId,
  onSelect,
  isExpanded,
  onFocusScope,
  onCreateProject,
  onOpenSettings,
  onCreateWorktree,
}: Props) {
  const width = isExpanded ? 248 : 70;
  const [orderedProjects, setOrderedProjects] = useState(projects);
  const [optimisticSelectedProjectId, setOptimisticSelectedProjectId] = useState(selectedProjectId);
  const optimisticProjectExists = orderedProjects.some((project) => project.id === optimisticSelectedProjectId);
  const displayedSelectedProjectId = optimisticProjectExists ? optimisticSelectedProjectId : selectedProjectId;

  const selectProject = useCallback(
    (id: string) => {
      if (displayedSelectedProjectId !== id) {
        setOptimisticSelectedProjectId(id);
      }
      onSelect(id);
    },
    [displayedSelectedProjectId, onSelect],
  );

  useEffect(() => {
    setOptimisticSelectedProjectId(selectedProjectId);
  }, [selectedProjectId]);

  useEffect(() => {
    setOrderedProjects((current) => mergeProjectOrder(current, projects));
  }, [projects]);

  const reorderProject = useCallback((sourceId: string, targetId: string) => {
    setOrderedProjects((current) => {
      const next = reorderById(current, sourceId, targetId);
      if (next === current) return current;
      if (window.__TAURI_INTERNALS__) {
        void invoke("project_reorder", {
          request: {
            projectIds: next.map((project) => project.id),
          },
        }).catch((error) => console.error("failed to reorder projects", error));
      }
      return next;
    });
  }, []);

  return (
    <nav
      className="h-full flex flex-col flex-shrink-0"
      style={{ width }}
      onPointerDown={onFocusScope}
      onFocusCapture={onFocusScope}
    >
      {isExpanded && (
        <div className="px-[18px] pt-[18px] pb-3 text-base font-bold tracking-tight text-ink">
          {tm("sidebar.workspace", "Workspace")}
        </div>
      )}

      <div className="flex-1 overflow-y-auto scrollbar-hidden">
        <div className={`flex flex-col ${isExpanded ? "gap-1 px-3 pt-1 pb-4" : "gap-1 px-3 pt-[18px] pb-4"}`}>
          {orderedProjects.map((project, index) => (
            <ProjectRow
              key={project.id}
              project={project}
              colors={
                project.badgeColorHex
                  ? projectBadgeColors(project.badgeColorHex)
                  : badgePalette[index % badgePalette.length]
              }
              isSelected={project.id === displayedSelectedProjectId}
              isExpanded={isExpanded}
              onPress={() => selectProject(project.id)}
              onReorder={reorderProject}
              onCreateWorktree={() => onCreateWorktree?.(project)}
              onEdit={() =>
                void openAppWindow("project-edit", {
                  projectId: project.id,
                  name: project.name,
                  path: project.path,
                  badgeSymbol: project.badgeSymbol ?? "",
                  badgeColorHex: project.badgeColorHex ?? "",
                })
              }
              onReveal={() => void revealProjectInFileManager(project.path)}
              onClose={() => {
                void closeProjectFromMenu(project).catch((error) => console.error("failed to close project", error));
              }}
            />
          ))}
        </div>
      </div>

      <div className={`flex flex-col gap-1.5 pb-4 pt-3 ${isExpanded ? "px-3" : "px-3 items-center"}`}>
        <FooterButton icon={Plus} label={t("addProject")} isExpanded={isExpanded} onPress={onCreateProject} />
        <FooterButton icon={Settings} label={t("settings")} isExpanded={isExpanded} onPress={onOpenSettings} />
        <FooterButton icon={MoreHorizontal} label={t("help")} isExpanded={isExpanded} menu />
      </div>
    </nav>
  );
}

function mergeProjectOrder(current: WorkspaceProject[], incoming: WorkspaceProject[]) {
  const incomingById = new Map(incoming.map((project) => [project.id, project]));
  const ordered = current
    .map((project) => incomingById.get(project.id))
    .filter((project): project is WorkspaceProject => Boolean(project));
  for (const project of incoming) {
    if (!current.some((item) => item.id === project.id)) {
      ordered.push(project);
    }
  }
  return ordered;
}

function reorderById<T extends { id: string }>(items: T[], sourceId: string, targetId: string) {
  const sourceIndex = items.findIndex((item) => item.id === sourceId);
  const targetIndex = items.findIndex((item) => item.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return items;
  const next = [...items];
  const [moved] = next.splice(sourceIndex, 1);
  next.splice(targetIndex, 0, moved);
  return next;
}

function ProjectRow({
  project,
  colors,
  isSelected,
  isExpanded,
  onPress,
  onCreateWorktree,
  onEdit,
  onReveal,
  onClose,
  onReorder,
}: {
  project: WorkspaceProject;
  colors: { from: string; to: string };
  isSelected: boolean;
  isExpanded: boolean;
  onPress: () => void;
  onCreateWorktree?: () => void;
  onEdit?: () => void;
  onReveal?: () => void;
  onClose?: () => void;
  onReorder?: (sourceId: string, targetId: string) => void;
}) {
  const initials = project.badge.slice(0, isExpanded ? 2 : 1).toUpperCase();
  const badgeSize = isExpanded ? 38 : 36;
  const BadgeIcon = project.badgeSymbol ? projectBadgeIcons[project.badgeSymbol] : undefined;
  const contextMenu = useContextMenu();

  const body = (
    <>
      <PressableButton
        onPressUp={onPress}
        onContextMenu={(event) => {
          contextMenu.openMenu(event);
        }}
        aria-busy={project.aiState === "running"}
        draggable={Boolean(onReorder)}
        onDragStart={(event) => {
          event.dataTransfer.effectAllowed = "move";
          event.dataTransfer.setData("text/plain", project.id);
        }}
        onDragOver={(event) => {
          if (!onReorder) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
        }}
        onDrop={(event) => {
          if (!onReorder) return;
          event.preventDefault();
          const sourceId = event.dataTransfer.getData("text/plain");
          if (!sourceId || sourceId === project.id) return;
          onReorder(sourceId, project.id);
        }}
        className={`w-full rounded-[12px] outline-none focus:outline-none focus-visible:outline-none ${
          isSelected ? "bg-fill/[0.085]" : "hover:bg-fill/[0.04]"
        }`}
        style={{ opacity: isSelected ? 1 : 0.82 }}
      >
        <div className={isExpanded ? "flex items-center gap-2.5 p-2" : "flex justify-center p-[6px]"}>
          <span
            className="relative rounded-[8px] grid place-items-center flex-shrink-0 shadow-badge"
            style={{
              width: badgeSize,
              height: badgeSize,
              background: `linear-gradient(135deg, ${colors.from} 0%, ${colors.to} 100%)`,
            }}
          >
            {project.aiState !== "idle" && <ProjectActivityBadge state={project.aiState} />}
            {BadgeIcon ? (
              <BadgeIcon size={isExpanded ? 17 : 16} className="text-on-brand" />
            ) : (
              <span className="text-on-brand font-bold text-sm tracking-tight">{initials}</span>
            )}
          </span>

          {isExpanded && (
            <div className="min-w-0 flex-1 text-left">
              <div className={`text-sm font-semibold truncate ${isSelected ? "text-ink" : "text-ink-soft"}`}>
                {project.name}
              </div>
              <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-xs font-medium text-ink-faint">
                {project.aiState === "running"
                  ? tm("agent.status.running", "Running")
                  : project.aiState === "review"
                    ? tm("project.activity.input_required", "Action required")
                    : project.aiState === "done"
                      ? tm("agent.status.completed", "Completed")
                      : project.path}
              </div>
            </div>
          )}
        </div>
      </PressableButton>
      <ContextMenu
        ariaLabel={formatI18n(tm("sidebar.project.actions_format", "%@ Actions"), project.name)}
        menu={contextMenu.menu}
        onClose={contextMenu.closeMenu}
      >
        <ContextMenuItem label={tm("worktree.create.title", "New Worktree")} onSelect={onCreateWorktree}>
          {tm("worktree.create.title", "New Worktree")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem label={tm("sidebar.project.edit", "Edit Project")} onSelect={onEdit}>
          {tm("sidebar.project.edit", "Edit Project")}
        </ContextMenuItem>
        <ContextMenuItem label={tm("sidebar.project.open_folder", "Open Folder")} onSelect={onReveal}>
          {tm("sidebar.project.open_folder", "Open Folder")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem label={tm("sidebar.project.remove", "Remove Project")} onSelect={onClose}>
          {tm("sidebar.project.remove", "Remove Project")}
        </ContextMenuItem>
      </ContextMenu>
    </>
  );

  if (!isExpanded) {
    return (
      <Tooltip label={project.name} placement="right" triggerClassName="block w-full">
        {body}
      </Tooltip>
    );
  }
  return body;
}

function ProjectActivityBadge({ state }: { state: WorkspaceProject["aiState"] }) {
  if (state === "idle") return null;

  if (state === "running") {
    return (
      <span className="absolute -right-0.5 -top-0.5 grid h-3.5 w-3.5 place-items-center rounded-full" aria-hidden="true">
        <span className="absolute inset-0 rounded-full border border-brand-amber/35 border-t-brand-amber motion-safe:animate-spin" />
        <span className="h-2 w-2 rounded-full bg-brand-amber" />
      </span>
    );
  }

  return (
    <span
      className={`absolute -right-0.5 -top-0.5 h-2.5 w-2.5 rounded-full border border-white ${
        state === "review" ? "bg-brand-amber" : "bg-brand-green"
      }`}
    />
  );
}

function FooterButton({
  icon: Icon,
  label,
  isExpanded,
  onPress,
  menu,
}: {
  icon: typeof Plus;
  label: string;
  isExpanded: boolean;
  onPress?: () => void;
  menu?: boolean;
}) {
  if (menu) {
    return <HelpMenuButton icon={Icon} label={label} isExpanded={isExpanded} />;
  }
  if (!isExpanded) {
    return (
      <Tooltip label={label} placement="right">
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          onPress={onPress}
          aria-label={label}
          className="h-7 w-7 min-w-7 focus:outline-none focus-visible:outline-none"
        >
          <Icon size={16} strokeWidth={1.7} />
        </Button>
      </Tooltip>
    );
  }
  return (
    <Button
      block
      size="md"
      variant="ghost"
      onPress={onPress}
      leading={Icon}
      className="h-8 justify-start font-medium text-ink-soft focus:outline-none focus-visible:outline-none"
    >
      {label}
    </Button>
  );
}

function HelpMenuButton({
  icon: Icon,
  label,
  isExpanded,
}: {
  icon: typeof MoreHorizontal;
  label: string;
  isExpanded: boolean;
}) {
  const [isOpen, setOpen] = useState(false);
  const trigger = isExpanded ? (
    <button
      type="button"
      className="h-8 w-full justify-start rounded-md px-2.5 text-sm font-medium text-ink-soft outline-none transition-colors hover:bg-fill/[0.06] hover:text-ink data-[pressed]:bg-fill/[0.08]"
      aria-label={label}
    >
      <span className="inline-flex min-w-0 items-center gap-2">
        <Icon size={14} strokeWidth={2} />
        <span className="truncate">{label}</span>
      </span>
    </button>
  ) : (
    <button
      type="button"
      className="grid h-7 w-7 place-items-center rounded-md text-ink-soft outline-none transition-colors hover:bg-fill/[0.06] hover:text-ink data-[pressed]:bg-fill/[0.08]"
      aria-label={label}
      title={label}
    >
      <Icon size={16} strokeWidth={1.7} />
    </button>
  );

  return (
    <DesktopMenu
      ariaLabel={tm("sidebar.footer.help", "Help")}
      isOpen={isOpen}
      onOpenChange={setOpen}
      placement={isExpanded ? "top-start" : "right-start"}
      trigger={trigger}
    >
      <DesktopMenuItem
        label={tm("menu.app.about_format", "About %@").replace("%@", "Codux")}
        onSelect={() => {
          setOpen(false);
          window.setTimeout(() => void showAbout(), 0);
        }}
      >
        {tm("menu.app.about_format", "About %@").replace("%@", "Codux")}
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("about.updates", "Check for Updates")} onSelect={() => void checkForUpdates()}>
        {tm("about.updates", "Check for Updates")}
      </DesktopMenuItem>
      <DesktopMenuItem
        label={tm("menu.help.export_diagnostics", "Export Diagnostics...")}
        onSelect={() => void exportDiagnostics()}
      >
        {tm("menu.help.export_diagnostics", "Export Diagnostics...")}
      </DesktopMenuItem>
      <DesktopMenuSeparator />
      <DesktopMenuItem
        label={tm("menu.help.open_runtime_log", "Open Runtime Log")}
        onSelect={() => void openRuntimeLog()}
      >
        {tm("menu.help.open_runtime_log", "Open Runtime Log")}
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("menu.help.open_live_log", "Open Live Log")} onSelect={() => void openLiveLog()}>
        {tm("menu.help.open_live_log", "Open Live Log")}
      </DesktopMenuItem>
      {import.meta.env.DEV && window.__TAURI_INTERNALS__ ? (
        <DesktopMenuItem
          label={tm("menu.help.developer_tools", "Developer Tools")}
          onSelect={() => void toggleDeveloperTools()}
        >
          {tm("menu.help.developer_tools", "Developer Tools")}
        </DesktopMenuItem>
      ) : null}
      <DesktopMenuSeparator />
      <DesktopMenuItem
        label={tm("menu.help.website", "Website")}
        onSelect={() => void openExternalUrl(CODUX_WEBSITE_URL)}
      >
        {tm("menu.help.website", "Website")}
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("menu.help.github", "GitHub")} onSelect={() => void openExternalUrl(CODUX_GITHUB_URL)}>
        {tm("menu.help.github", "GitHub")}
      </DesktopMenuItem>
    </DesktopMenu>
  );
}
