import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, getCurrentWindow } from "@tauri-apps/api/window";
import { formatI18n, tm } from "./i18n";

export type AppWindowKind =
  | "settings"
  | "project-create"
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
  settings: {
    label: "settings",
    titleKey: "menu.settings",
    titleFallback: "Settings",
    width: 820,
    height: 640,
    minWidth: 720,
    minHeight: 540,
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
  "desktop-pet": {
    label: "desktop-pet",
    titleKey: "settings.pet.desktop_widget",
    titleFallback: "Desktop Pet",
    width: 352,
    height: 218,
    minWidth: 220,
    minHeight: 140,
    route: "/desktop-pet",
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
    width: 760,
    height: 620,
    minWidth: 680,
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

export async function openAppWindow(kind: AppWindowKind) {
  if (!window.__TAURI_INTERNALS__) {
    window.location.hash = windowConfig[kind].route;
    return;
  }

  const config = windowConfig[kind];
  const existing = await WebviewWindow.getByLabel(config.label);
  if (existing) {
    try {
      await existing.show();
      await existing.setFocus();
    } catch (error) {
      console.error(`failed to reveal ${kind} window`, error);
    }
    return;
  }

  const appWindow = new WebviewWindow(config.label, {
    title: tm(config.titleKey, config.titleFallback),
    url: `/#${config.route}`,
    width: config.width,
    height: config.height,
    minWidth: config.minWidth,
    minHeight: config.minHeight,
    resizable: kind === "desktop-pet" || kind === "pet-claim" || kind === "pet-custom-install" ? false : true,
    transparent: kind === "desktop-pet",
    decorations: kind === "desktop-pet" ? false : true,
    titleBarStyle: kind === "desktop-pet" ? undefined : "overlay",
    hiddenTitle: kind === "desktop-pet" ? undefined : true,
    acceptFirstMouse: true,
    trafficLightPosition: kind === "desktop-pet" ? undefined : new LogicalPosition(14, 22),
    backgroundColor: kind === "desktop-pet" ? "#00000000" : opaqueAppWindowBackground,
    visible: false,
    focus: false,
    skipTaskbar: kind === "desktop-pet" ? true : undefined,
    alwaysOnTop: kind === "desktop-pet" ? true : undefined,
  });

  appWindow.once("tauri://error", (event) => {
    console.error(`failed to create ${kind} window`, event.payload);
  });
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
    acceptFirstMouse: true,
    backgroundColor: "#171b22",
    visible: false,
    focus: false,
  });

  appWindow.once("tauri://error", (event) => {
    console.error("failed to create detached terminal window", event.payload);
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
    acceptFirstMouse: true,
    trafficLightPosition: new LogicalPosition(14, 22),
    backgroundColor: opaqueAppWindowBackground,
    visible: false,
    focus: false,
  });

  appWindow.once("tauri://error", (event) => {
    console.error("failed to create git diff window", event.payload);
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
  const currentWindow = getCurrentWebviewWindow();
  await currentWindow.show();
  await currentWindow.setFocus();
}

export async function closeCurrentAppWindow() {
  if (!window.__TAURI_INTERNALS__) {
    window.location.hash = "";
    return;
  }
  await getCurrentWindow().close();
}

export async function destroyCurrentAppWindow() {
  if (!window.__TAURI_INTERNALS__) {
    window.location.hash = "";
    return;
  }
  await getCurrentWindow().destroy();
}
