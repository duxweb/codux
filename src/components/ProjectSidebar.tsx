import {
  ArrowTopRight,
  Box,
  Bug,
  Download,
  FileArchive,
  FileText,
  Info,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Server,
  Settings,
  TerminalSquare,
} from "../icons";
import {
  checkForUpdates,
  exportDiagnostics,
  openExternalUrl,
  openLiveLog,
  openRuntimeLog,
  showAbout,
  toggleDeveloperTools,
} from "../appActions";
import { CODUX_GITHUB_URL, CODUX_WEBSITE_URL } from "../appLinks";
import { useState } from "react";
import { t, tm } from "../i18n";
import { Button } from "./Button";
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
  folder: FileArchive,
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
}: Props) {
  const width = isExpanded ? 248 : 70;

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
        <div
          className={`flex flex-col ${
            isExpanded ? "gap-1 px-3 pt-1 pb-4" : "gap-1 px-3 pt-[18px] pb-4"
          }`}
        >
          {projects.map((project, index) => (
            <ProjectRow
              key={project.id}
              project={project}
              colors={project.badgeColorHex ? projectBadgeColors(project.badgeColorHex) : badgePalette[index % badgePalette.length]}
              isSelected={project.id === selectedProjectId}
              isExpanded={isExpanded}
              onPress={() => onSelect(project.id)}
            />
          ))}
        </div>
      </div>

      <div
        className={`flex flex-col gap-1.5 pb-4 pt-3 ${
          isExpanded ? "px-3" : "px-3 items-center"
        }`}
      >
        <FooterButton
          icon={Plus}
          label={t("addProject")}
          isExpanded={isExpanded}
          onPress={onCreateProject}
        />
        <FooterButton
          icon={Settings}
          label={t("settings")}
          isExpanded={isExpanded}
          onPress={onOpenSettings}
        />
        <FooterButton
          icon={MoreHorizontal}
          label={t("help")}
          isExpanded={isExpanded}
          menu
        />
      </div>
    </nav>
  );
}

function ProjectRow({
  project,
  colors,
  isSelected,
  isExpanded,
  onPress,
}: {
  project: WorkspaceProject;
  colors: { from: string; to: string };
  isSelected: boolean;
  isExpanded: boolean;
  onPress: () => void;
}) {
  const initials = project.badge.slice(0, isExpanded ? 2 : 1).toUpperCase();
  const badgeSize = isExpanded ? 38 : 36;
  const BadgeIcon = project.badgeSymbol ? projectBadgeIcons[project.badgeSymbol] : undefined;

  const body = (
    <PressableButton
      onPressUp={onPress}
      aria-busy={project.aiState === "running"}
      className={`w-full rounded-[12px] outline-none focus:outline-none focus-visible:outline-none transition-colors ${
        isSelected ? "bg-fill/[0.085]" : "hover:bg-fill/[0.04]"
      }`}
      style={{ opacity: isSelected ? 1 : 0.82 }}
    >
      <div
        className={
          isExpanded
            ? "flex items-center gap-2.5 p-2"
            : "flex justify-center p-[6px]"
        }
      >
        <span
          className="relative rounded-[8px] grid place-items-center flex-shrink-0 shadow-badge"
          style={{
            width: badgeSize,
            height: badgeSize,
            background: `linear-gradient(135deg, ${colors.from} 0%, ${colors.to} 100%)`,
          }}
        >
          {project.aiState !== "idle" && (
            project.aiState === "running" ? (
              <span className="absolute -right-1 -top-1 grid h-4 w-4 place-items-center rounded-full bg-surface-chrome ring-2 ring-surface-chrome">
                <RefreshCw size={10} className="animate-spin text-brand-amber" />
              </span>
            ) : (
              <span
                className={`absolute -right-1 -top-1 h-2.5 w-2.5 rounded-full ring-2 ring-surface-chrome ${
                  project.aiState === "review"
                    ? "bg-brand-blue"
                    : "bg-brand-green"
                }`}
              />
            )
          )}
          {BadgeIcon ? (
            <BadgeIcon size={isExpanded ? 17 : 16} className="text-on-brand" />
          ) : (
            <span className="text-on-brand font-bold text-sm tracking-tight">
              {initials}
            </span>
          )}
        </span>

        {isExpanded && (
          <div className="min-w-0 flex-1 text-left">
            <div
              className={`text-sm font-semibold truncate ${
                isSelected ? "text-ink" : "text-ink-soft"
              }`}
            >
              {project.name}
            </div>
            <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-xs font-medium text-ink-faint">
              {project.aiState === "running" && (
                <RefreshCw size={11} className="flex-shrink-0 animate-spin text-brand-amber" />
              )}
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
      placement={isExpanded ? "top-start" : "right-end"}
      trigger={trigger}
    >
      <DesktopMenuItem label={tm("menu.app.about_format", "About %@").replace("%@", "Codux")} onSelect={() => void showAbout()}>
        <MenuItem icon={Info} label={tm("menu.app.about_format", "About %@").replace("%@", "Codux")} />
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("about.updates", "Check for Updates")} onSelect={() => void checkForUpdates()}>
        <MenuItem icon={Download} label={tm("about.updates", "Check for Updates")} />
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("menu.help.export_diagnostics", "Export Diagnostics...")} onSelect={() => void exportDiagnostics()}>
        <MenuItem icon={FileArchive} label={tm("menu.help.export_diagnostics", "Export Diagnostics...")} />
      </DesktopMenuItem>
      <DesktopMenuSeparator />
      <DesktopMenuItem label={tm("menu.help.open_runtime_log", "Open Runtime Log")} onSelect={() => void openRuntimeLog()}>
        <MenuItem icon={FileText} label={tm("menu.help.open_runtime_log", "Open Runtime Log")} />
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("menu.help.open_live_log", "Open Live Log")} onSelect={() => void openLiveLog()}>
        <MenuItem icon={Bug} label={tm("menu.help.open_live_log", "Open Live Log")} />
      </DesktopMenuItem>
      {import.meta.env.DEV && (
        <DesktopMenuItem label={tm("menu.help.developer_tools", "Developer Tools")} onSelect={() => void toggleDeveloperTools()}>
          <MenuItem icon={Bug} label={tm("menu.help.developer_tools", "Developer Tools")} />
        </DesktopMenuItem>
      )}
      <DesktopMenuSeparator />
      <DesktopMenuItem label={tm("menu.help.website", "Website")} onSelect={() => void openExternalUrl(CODUX_WEBSITE_URL)}>
        <MenuItem icon={ArrowTopRight} label={tm("menu.help.website", "Website")} />
      </DesktopMenuItem>
      <DesktopMenuItem label={tm("menu.help.github", "GitHub")} onSelect={() => void openExternalUrl(CODUX_GITHUB_URL)}>
        <MenuItem icon={ArrowTopRight} label={tm("menu.help.github", "GitHub")} />
      </DesktopMenuItem>
    </DesktopMenu>
  );
}

function MenuItem({
  icon: Icon,
  label,
}: {
  icon: typeof Info;
  label: string;
}) {
  return (
    <span className="inline-flex items-center gap-2">
      <Icon size={13} strokeWidth={2} />
      <span>{label}</span>
    </span>
  );
}
