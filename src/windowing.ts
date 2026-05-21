import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import { formatI18n, tm } from "./i18n";
import { isMacPlatform } from "./platform";

export type AppWindowKind =
  | "about"
  | "settings"
  | "project-create"
  | "project-edit"
  | "desktop-pet"
  | "pet-claim"
  | "pet-dex"
  | "pet-custom-install"
  | "memory-manager";

export type GitDiffWindowOptions = {
  projectPath: string;
  path: string;
  staged?: boolean;
};

type DetachedTerminalWindowOptions = {
  terminalId: string;
  backendId: string;
  projectId: string;
  slotId: string;
  paneId: string;
  title: string;
  cwd: string;
  projectName?: string;
};

type WindowConfig = {
  label: string;
  titleKey: string;
  titleFallback: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
  route: string;
};

const windowConfig: Record<AppWindowKind, WindowConfig> = {
  about: {
    label: "about",
    titleKey: "menu.app.about_format",
    titleFallback: "About Codux",
    width: 320,
    height: 380,
    minWidth: 320,
    minHeight: 380,
    route: "/about",
  },
  settings: {
    label: "settings",
    titleKey: "menu.settings",
    titleFallback: "Settings",
    width: 780,
    height: 640,
    minWidth: 720,
    minHeight: 520,
    route: "/settings",
  },
  "project-create": {
    label: "project-create",
    titleKey: "project.create.title",
    titleFallback: "Create Project",
    width: 600,
    height: 552,
    minWidth: 540,
    minHeight: 552,
    route: "/project-create",
  },
  "project-edit": {
    label: "project-edit",
    titleKey: "project.edit.title",
    titleFallback: "Edit Project",
    width: 600,
    height: 552,
    minWidth: 540,
    minHeight: 552,
    route: "/project-create",
  },
  "desktop-pet": {
    label: "desktop-pet",
    titleKey: "settings.pet.desktop_widget",
    titleFallback: "Desktop Pet",
    width: 352,
    height: 218,
    minWidth: 220,
    minHeight: 140,
    route: "desktop-pet.html",
  },
  "pet-claim": {
    label: "pet-claim",
    titleKey: "pet.claim.window.title",
    titleFallback: "Claim Pet",
    width: 680,
    height: 500,
    minWidth: 640,
    minHeight: 460,
    route: "/pet-claim",
  },
  "pet-dex": {
    label: "pet-dex",
    titleKey: "pet.dex.title",
    titleFallback: "Petdex",
    width: 900,
    height: 660,
    minWidth: 780,
    minHeight: 560,
    route: "/pet-dex",
  },
  "pet-custom-install": {
    label: "pet-custom-install",
    titleKey: "pet.custom.install.title",
    titleFallback: "Add Custom Pet",
    width: 680,
    height: 210,
    minWidth: 620,
    minHeight: 210,
    route: "/pet-custom-install",
  },
  "memory-manager": {
    label: "memory-manager",
    titleKey: "memory.manager.window.title",
    titleFallback: "Memory Manager",
    width: 940,
    height: 660,
    minWidth: 820,
    minHeight: 560,
    route: "/memory-manager",
  },
};

const opaqueAppWindowBackground = "#22262e";
const childWindowTrafficLightPosition = () => (isMacPlatform() ? new LogicalPosition(14, 21) : undefined);

export async function openAppWindow(kind: AppWindowKind, query?: Record<string, string | null | undefined>) {
  if (!window.__TAURI_INTERNALS__) {
    window.location.hash = routeWithQuery(windowConfig[kind].route, query);
    return;
  }

  const config = windowConfig[kind];
  const label = windowLabel(kind, query);
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    try {
      await existing.show();
      await existing.setFocus();
    } catch (error) {
      console.error(`failed to reveal ${kind} window`, error);
    }
    return;
  }

  const appWindow = new WebviewWindow(label, {
    title: tm(config.titleKey, config.titleFallback).replace("%@", "Codux"),
    url: kind === "desktop-pet" ? config.route : `/#${routeWithQuery(config.route, query)}`,
    width: config.width,
    height: config.height,
    minWidth: config.minWidth,
    minHeight: config.minHeight,
    resizable:
      kind === "about" || kind === "desktop-pet" || kind === "pet-claim" || kind === "pet-custom-install"
        ? false
        : true,
    transparent: kind === "desktop-pet",
    decorations: kind === "desktop-pet" ? false : true,
    titleBarStyle: kind === "desktop-pet" ? undefined : "overlay",
    hiddenTitle: kind === "desktop-pet" ? undefined : true,
    acceptFirstMouse: true,
    trafficLightPosition: kind === "desktop-pet" ? undefined : childWindowTrafficLightPosition(),
    backgroundColor: kind === "desktop-pet" ? "#00000000" : opaqueAppWindowBackground,
    visible: false,
    focus: false,
    skipTaskbar: kind === "desktop-pet" ? true : undefined,
    alwaysOnTop: kind === "desktop-pet" ? true : undefined,
  });

  appWindow.once("tauri://error", (event) => {
    console.error(`failed to create ${kind} window`, event.payload);
  });
  appWindow.once("tauri://created", () => {
    void appWindow
      .show()
      .then(() => appWindow.setFocus())
      .catch((error) => {
        console.error(`failed to reveal ${kind} window`, error);
      });
  });
}

function routeWithQuery(route: string, query?: Record<string, string | null | undefined>) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value != null) params.set(key, value);
  }
  const suffix = params.toString();
  return suffix ? `${route}?${suffix}` : route;
}

function windowLabel(kind: AppWindowKind, query?: Record<string, string | null | undefined>) {
  if (kind === "project-edit" && query?.projectId) {
    return `project-edit-${query.projectId}`;
  }
  return windowConfig[kind].label;
}

export async function openDetachedTerminalWindow(options: DetachedTerminalWindowOptions) {
  const route = `/terminal?terminalId=${encodeURIComponent(options.terminalId)}&backendId=${encodeURIComponent(options.backendId)}&projectId=${encodeURIComponent(options.projectId)}&slotId=${encodeURIComponent(options.slotId)}&paneId=${encodeURIComponent(options.paneId)}&title=${encodeURIComponent(options.title)}&cwd=${encodeURIComponent(options.cwd)}&projectName=${encodeURIComponent(options.projectName ?? "")}`;

  if (!window.__TAURI_INTERNALS__) {
    window.open(`#${route}`, "_blank");
    return;
  }

  const label = `terminal-${options.terminalId}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    try {
      await existing.show();
      await existing.setFocus();
    } catch (error) {
      console.error("failed to reveal detached terminal window", error);
    }
    return;
  }

  const appWindow = new WebviewWindow(label, {
    title: options.title,
    url: `/#${route}`,
    width: 920,
    height: 580,
    minWidth: 560,
    minHeight: 360,
    resizable: true,
    transparent: false,
    decorations: true,
    titleBarStyle: "overlay",
    hiddenTitle: true,
    trafficLightPosition: childWindowTrafficLightPosition(),
    acceptFirstMouse: true,
    backgroundColor: "#171b22",
    visible: false,
    focus: false,
  });

  appWindow.once("tauri://error", (event) => {
    console.error("failed to create detached terminal window", event.payload);
  });
  appWindow.once("tauri://created", () => {
    void appWindow
      .show()
      .then(() => appWindow.setFocus())
      .catch((error) => {
        console.error("failed to reveal detached terminal window", error);
      });
  });
}

export async function openGitDiffWindow(options: GitDiffWindowOptions) {
  const route = `/git-diff?projectPath=${encodeURIComponent(options.projectPath)}&path=${encodeURIComponent(options.path)}&staged=${options.staged ? "1" : "0"}`;

  if (!window.__TAURI_INTERNALS__) {
    window.open(`#${route}`, "_blank");
    return;
  }

  const label = `git-diff-${stableWindowSegment(options.projectPath)}-${stableWindowSegment(options.path)}-${options.staged ? "staged" : "worktree"}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    try {
      await existing.show();
      await existing.setFocus();
    } catch (error) {
      console.error("failed to reveal git diff window", error);
    }
    return;
  }

  const appWindow = new WebviewWindow(label, {
    title: formatI18n(tm("git.diff.window.title_format", "Diff - %@"), options.path),
    url: `/#${route}`,
    width: 1040,
    height: 720,
    minWidth: 760,
    minHeight: 480,
    resizable: true,
    transparent: false,
    decorations: true,
    titleBarStyle: "overlay",
    hiddenTitle: true,
    trafficLightPosition: childWindowTrafficLightPosition(),
    acceptFirstMouse: true,
    backgroundColor: opaqueAppWindowBackground,
    visible: false,
    focus: false,
  });

  appWindow.once("tauri://error", (event) => {
    console.error("failed to create git diff window", event.payload);
  });
  appWindow.once("tauri://created", () => {
    void appWindow
      .show()
      .then(() => appWindow.setFocus())
      .catch((error) => {
        console.error("failed to reveal git diff window", error);
      });
  });
}

function stableWindowSegment(value: string) {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = (hash * 31 + value.charCodeAt(index)) >>> 0;
  }
  return hash.toString(36);
}

export async function revealCurrentAppWindow() {
  if (!window.__TAURI_INTERNALS__) return;
  const currentWindow = getCurrentWebviewWindow();
  await currentWindow.show();
}

export async function revealMainAppWindow() {
  if (!window.__TAURI_INTERNALS__) return;
  const mainWindow = await WebviewWindow.getByLabel("main");
  if (!mainWindow) return;
  await mainWindow.show();
  await mainWindow.setFocus();
}

export async function closeCurrentAppWindow() {
  if (!window.__TAURI_INTERNALS__) {
    window.location.hash = "";
    return;
  }
  await getCurrentWindow().close();
}

export async function resizeCurrentAppWindow(width: number, height: number) {
  if (!window.__TAURI_INTERNALS__) return;
  await getCurrentWindow().setSize(new LogicalSize(width, height));
}

export async function destroyCurrentAppWindow() {
  if (!window.__TAURI_INTERNALS__) {
    window.location.hash = "";
    return;
  }
  await getCurrentWindow().destroy();
}
