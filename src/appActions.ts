import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { tm } from "./i18n";
import { openLocalizedDialog, saveLocalizedDialog } from "./localizedDialog";
import { systemConfirm, systemMessage } from "./systemDialog";
import type { WorkspaceProject } from "./types";
import { openAppWindow } from "./windowing";
import { dispatchWorkspaceCommand, type WorkspaceCommand } from "./workspaceCommands";

export type AppAboutMetadata = {
  name: string;
  version: string;
  identifier: string;
  description: string;
  targetOs: string;
  targetArch: string;
  buildProfile: string;
};

export type UpdateStatus = {
  configured: boolean;
  checking: boolean;
  available: boolean;
  automaticInstallSupported: boolean;
  signedUpdaterConfigured: boolean;
  manifestEndpointConfigured: boolean;
  currentVersion: string;
  latestVersion?: string | null;
  downloadUrl?: string | null;
  notes?: string | null;
  channel?: string | null;
  installationMode: string;
  message: string;
};

export type DiagnosticsExportResult = {
  path: string;
  bytes: number;
};

export type UpdateInstallResult = {
  installed: boolean;
  version?: string | null;
  downloadedBytes: number;
  totalBytes?: number | null;
  message: string;
};

export async function showAbout() {
  await openAppWindow("about");
}

export async function checkForUpdates() {
  const status = window.__TAURI_INTERNALS__
    ? await invoke<UpdateStatus>("app_update_status")
    : {
        configured: false,
        checking: false,
        available: false,
        automaticInstallSupported: false,
        signedUpdaterConfigured: false,
        manifestEndpointConfigured: false,
        currentVersion: fallbackAbout().version,
        latestVersion: null,
        downloadUrl: null,
        notes: null,
        channel: null,
        installationMode: "preview",
        message: tm("update.not_configured.preview", "Update channel is not configured in browser preview."),
      };

  if (!status.configured) {
    await systemMessage(status.message, {
      title: tm("update.not_configured.title", "Updates Not Configured"),
      kind: "info",
      buttons: { ok: "OK" },
    });
    return;
  }

  if (status.available) {
    const notes = status.notes?.trim();
    const message = [
      tm("update.available.message_format", "A new version v%@ is available. You are currently using v%@.")
        .replace("%@", status.latestVersion ?? status.currentVersion)
        .replace("%@", status.currentVersion),
      notes ? `${tm("update.available.notes_title", "Release Notes")}\n${notes}` : "",
    ]
      .filter(Boolean)
      .join("\n\n");
    const shouldInstall = await systemConfirm(message, {
      title: tm("update.available.title", "Update Available"),
      kind: "info",
      okLabel: status.automaticInstallSupported
        ? tm("update.available.install", "Install")
        : tm("update.available.open", "Download"),
      cancelLabel: tm("update.available.later", "Later"),
    });
    if (!shouldInstall) return;
    if (status.automaticInstallSupported && window.__TAURI_INTERNALS__) {
      const result = await invoke<UpdateInstallResult>("app_update_install");
      await systemMessage(tm("update.installed.message", result.message), {
        title: tm("update.installed.title", "Update Installed"),
        kind: "info",
        buttons: { ok: "OK" },
      });
      return;
    }
    if (status.downloadUrl) {
      await openExternalUrl(status.downloadUrl);
    }
    return;
  }

  await systemMessage(
    tm("update.latest.message_format", "Current version: v%@\nLatest release: v%@")
      .replace("%@", status.currentVersion)
      .replace("%@", status.latestVersion ?? status.currentVersion),
    {
      title: tm("update.latest.title", "You're up to date."),
      kind: "info",
      buttons: { ok: "OK" },
    },
  );
}

export async function exportDiagnostics() {
  if (!window.__TAURI_INTERNALS__) {
    await systemMessage("Diagnostics export is only available in the desktop app.", {
      title: tm("diagnostics.export.error.title", "Unable to Export Diagnostics"),
      kind: "warning",
    });
    return;
  }
  const defaultPath = `codux-diagnostics-${timestampSlug()}.json`;
  const destinationPath = await saveLocalizedDialog({
    title: tm("diagnostics.export.panel.title", "Export Diagnostics"),
    prompt: tm("common.save", "Save"),
    defaultPath,
    filters: [{ name: tm("diagnostics.export.filter.json", "JSON"), extensions: ["json"] }],
  });
  if (!destinationPath) return;
  const result = await invoke<DiagnosticsExportResult>("diagnostics_export", {
    request: { destinationPath },
  });
  await systemMessage(
    tm("diagnostics.export.success_format", "Exported diagnostics to %@.").replace("%@", result.path),
    {
      title: tm("diagnostics.export.panel.title", "Export Diagnostics"),
      kind: "info",
      buttons: { ok: "OK" },
    },
  );
}

export async function openRuntimeLog() {
  if (window.__TAURI_INTERNALS__) {
    await invoke("app_open_runtime_log");
  }
}

export async function openLiveLog() {
  if (window.__TAURI_INTERNALS__) {
    await invoke("app_open_live_log");
  }
}

export async function openExternalUrl(url: string) {
  if (window.__TAURI_INTERNALS__) {
    await invoke("app_open_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function toggleDeveloperTools() {
  if (window.__TAURI_INTERNALS__) {
    await invoke("app_toggle_devtools");
  }
}

export async function openProjectFolderFromMenu() {
  if (!window.__TAURI_INTERNALS__) return null;
  if (openProjectFolderRequest) return openProjectFolderRequest;
  openProjectFolderRequest = openProjectFolderFromMenuUnsafe().finally(() => {
    openProjectFolderRequest = null;
  });
  return openProjectFolderRequest;
}

let openProjectFolderRequest: Promise<boolean | null> | null = null;

async function openProjectFolderFromMenuUnsafe() {
  const selected = await openLocalizedDialog({
    directory: true,
    multiple: false,
    title: tm("project.open_folder.title", "Open Folder"),
    message: tm("project.open_folder.message", "Choose a project folder to import."),
    prompt: tm("project.open_folder.prompt", "Open"),
  });
  if (typeof selected !== "string") return null;
  await invoke("project_create", {
    request: {
      name: selected.split(/[\\/]/).filter(Boolean).pop() ?? "Project",
      path: selected,
      badgeText: null,
      badgeSymbol: null,
      badgeColorHex: null,
    },
  });
  return true;
}

function installMenuListener<T>(
  unlisteners: Array<() => void>,
  isDisposed: () => boolean,
  event: string,
  handler: (payload: T) => void,
) {
  void listen<T>(event, ({ payload }) => {
    if (!isDisposed()) handler(payload);
  }).then((unlisten) => {
    if (isDisposed()) {
      unlisten();
      return;
    }
    unlisteners.push(unlisten);
  });
}

export async function closeProjectFromMenu(project?: WorkspaceProject) {
  if (!window.__TAURI_INTERNALS__ || (!project?.rootProjectId && !project?.id)) return null;
  const projectId = project.rootProjectId ?? project.id;
  await invoke("project_close", {
    request: { projectId },
  });
  return true;
}

export async function closeAllProjectsFromMenu(projects: WorkspaceProject[]) {
  if (!window.__TAURI_INTERNALS__ || projects.length === 0) return null;
  const confirmed = await systemConfirm(
    tm(
      "workspace.close_all_projects.message",
      "Are you sure you want to close all projects in the current workspace? Files on disk will not be deleted.",
    ),
    {
      title: tm("workspace.close_all_projects.title", "Close All Projects"),
      kind: "warning",
      okLabel: tm("workspace.close_all_projects.confirm", "Close All"),
      cancelLabel: tm("common.cancel", "Cancel"),
    },
  );
  if (!confirmed) return null;
  await invoke("project_close_all");
  return true;
}

export function installAppMenuActions() {
  if (!window.__TAURI_INTERNALS__) return () => {};
  let disposed = false;
  const unlisteners: Array<() => void> = [];
  const isDisposed = () => disposed;
  installMenuListener<void>(unlisteners, isDisposed, "app-menu:settings", () => void openAppWindow("settings"));
  installMenuListener<void>(
    unlisteners,
    isDisposed,
    "app-menu:project-create",
    () => void openAppWindow("project-create"),
  );
  installMenuListener<void>(unlisteners, isDisposed, "app-menu:about", () => void showAbout());
  installMenuListener<void>(unlisteners, isDisposed, "app-menu:check-updates", () => void checkForUpdates());
  installMenuListener<void>(unlisteners, isDisposed, "app-menu:export-diagnostics", () => void exportDiagnostics());
  return () => {
    disposed = true;
    unlisteners.splice(0).forEach((unlisten) => unlisten());
  };
}

export function installWorkspaceMenuActions(handlers: {
  setMainView: (view: "terminal" | "files" | "review") => void;
  toggleProjects: () => void;
  toggleTasks: () => void;
  toggleRightPanel: (panel: "git" | "files" | "ai" | "ssh") => void;
  createTask: () => void;
  openProjectFolder: () => void;
  closeCurrentProject: () => void;
  closeAllProjects: () => void;
}) {
  if (!window.__TAURI_INTERNALS__) return () => {};
  let disposed = false;
  const unlisteners: Array<() => void> = [];
  const isDisposed = () => disposed;
  const install = <T>(event: string, handler: (payload: T) => void) => {
    installMenuListener(unlisteners, isDisposed, event, handler);
  };

  install<"terminal" | "files" | "review">("app-menu:view", handlers.setMainView);
  install<"projects" | "tasks">("app-menu:toggle-sidebar", (payload) => {
    if (payload === "projects") handlers.toggleProjects();
    if (payload === "tasks") handlers.toggleTasks();
  });
  install<"git" | "files" | "ai" | "ssh">("app-menu:right-panel", handlers.toggleRightPanel);
  install("app-menu:project-open-folder", handlers.openProjectFolder);
  install("app-menu:project-close-current", handlers.closeCurrentProject);
  install("app-menu:project-close-all", handlers.closeAllProjects);
  install<"add-top-terminal-split" | "add-bottom-terminal-tab">("app-menu:workspace-command", (payload) => {
    const command: WorkspaceCommand =
      payload === "add-top-terminal-split" ? { type: "add-top-terminal-split" } : { type: "add-bottom-terminal-tab" };
    dispatchWorkspaceCommand(command);
  });
  install<"editor-save" | "editor-search" | "close-active">("app-menu:workspace-command", (payload) => {
    const command: WorkspaceCommand = { type: payload };
    dispatchWorkspaceCommand(command);
  });
  install("app-menu:task-create", handlers.createTask);

  return () => {
    disposed = true;
    unlisteners.splice(0).forEach((unlisten) => unlisten());
  };
}

function timestampSlug() {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(
    now.getHours(),
  )}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function fallbackAbout(): AppAboutMetadata {
  return {
    name: "Codux",
    version: "0.1.0",
    identifier: "com.duxweb.dmux",
    description: "Codux desktop workspace",
    targetOs: "web",
    targetArch: "browser",
    buildProfile: "preview",
  };
}
