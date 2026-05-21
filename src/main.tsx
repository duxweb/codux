import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { installDesktopBrowserBehavior } from "./desktopBehavior";
import { lockRuntimeLocale, syncI18nBundleFromRust } from "./i18n";
import { readAppSettings, subscribeAppSettings, syncAppSettingsFromRust } from "./settings";
import { preloadRuntimeSnapshots } from "./startupPreload";
import { applyConfiguredTheme, initSystemTheme } from "./theme";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

const uninstallDesktopBrowserBehavior = installDesktopBrowserBehavior();
const uninstallDevtoolsShortcut = installDevtoolsShortcut();

const route = window.location.hash.replace(/^#/, "");
const routePath = route.split("?")[0] || route;
const isStandalone =
  routePath === "/about" ||
  routePath === "/settings" ||
  routePath === "/project-create" ||
  routePath === "/pet-claim" ||
  routePath === "/pet-dex" ||
  routePath === "/pet-custom-install" ||
  routePath === "/memory-manager" ||
  route.startsWith("/terminal") ||
  route.startsWith("/git-diff");
if (isStandalone) {
  document.documentElement.classList.add("standalone-window");
}
if (route.startsWith("/terminal")) {
  document.documentElement.classList.add("terminal-window");
}

let runtimeThemeSettings = readAppSettings();

async function loadRoot() {
  if (routePath === "/about") {
    const { AboutWindow } = await import("./windows/AboutWindow");
    return AboutWindow;
  }
  if (routePath === "/settings") {
    const { SettingsWindow } = await import("./windows/SettingsWindow");
    return SettingsWindow;
  }
  if (routePath === "/project-create") {
    const { ProjectCreateWindow } = await import("./windows/ProjectCreateWindow");
    return ProjectCreateWindow;
  }
  if (routePath === "/pet-claim") {
    const { PetClaimWindow } = await import("./windows/PetClaimWindow");
    return PetClaimWindow;
  }
  if (routePath === "/pet-dex") {
    const { PetDexWindow } = await import("./windows/PetDexWindow");
    return PetDexWindow;
  }
  if (routePath === "/pet-custom-install") {
    const { PetCustomPetInstallWindow } = await import("./windows/PetCustomPetInstallWindow");
    return PetCustomPetInstallWindow;
  }
  if (routePath === "/memory-manager") {
    const { MemoryManagerWindow } = await import("./windows/MemoryManagerWindow");
    return MemoryManagerWindow;
  }
  if (route.startsWith("/terminal")) {
    const { DetachedTerminalWindow } = await import("./windows/DetachedTerminalWindow");
    return DetachedTerminalWindow;
  }
  if (route.startsWith("/git-diff")) {
    const { GitDiffWindow } = await import("./windows/GitDiffWindow");
    return GitDiffWindow;
  }
  const { default: App } = await import("./App");
  return App;
}

const uninstallSystemTheme = initSystemTheme(() => runtimeThemeSettings);
const uninstallSettingsThemeSync = subscribeAppSettings((settings) => {
  const nextRuntimeThemeSettings = {
    ...settings,
    language: runtimeThemeSettings.language,
    theme: runtimeThemeSettings.theme,
  };
  runtimeThemeSettings = nextRuntimeThemeSettings;
  applyConfiguredTheme(runtimeThemeSettings);
});
syncInitialThemeAndLocale();
const reactRoot = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
renderStartupShell();

void bootstrapRoot()
  .then((Root) => {
    reactRoot.render(
      <React.StrictMode>
        <StartupWindowReveal />
        <Root />
      </React.StrictMode>,
    );
  })
  .catch((error) => {
    console.error("failed to load application", error);
    reactRoot.render(<StartupError />);
  });

const uninstallAppRuntime = () => {
  uninstallDesktopBrowserBehavior();
  uninstallDevtoolsShortcut();
  uninstallSystemTheme();
  uninstallSettingsThemeSync();
};

window.addEventListener("beforeunload", uninstallAppRuntime, { once: true });
if (import.meta.hot) {
  import.meta.hot.dispose(uninstallAppRuntime);
}

function syncInitialThemeAndLocale() {
  runtimeThemeSettings = readAppSettings();
  applyConfiguredTheme(runtimeThemeSettings);
  lockRuntimeLocale(runtimeThemeSettings);
  return runtimeThemeSettings;
}

function installDevtoolsShortcut() {
  if (!window.__TAURI_INTERNALS__) return () => undefined;
  const handleKeyDown = (event: KeyboardEvent) => {
    const isToggle =
      event.key === "F12" || (event.key.toLowerCase() === "i" && event.ctrlKey && event.shiftKey);
    if (!isToggle) return;
    event.preventDefault();
    event.stopPropagation();
    void invoke("app_toggle_devtools");
  };
  window.addEventListener("keydown", handleKeyDown, true);
  return () => window.removeEventListener("keydown", handleKeyDown, true);
}

async function syncStartupResources() {
  try {
    const [settings] = await Promise.all([syncAppSettingsFromRust(), syncI18nBundleFromRust()]);
    runtimeThemeSettings = settings;
    applyConfiguredTheme(runtimeThemeSettings);
    lockRuntimeLocale(runtimeThemeSettings);
  } catch (error) {
    console.error("failed to sync startup resources", error);
  }
}

async function bootstrapRoot() {
  const preloadPromise = isStandalone ? Promise.resolve() : preloadRuntimeSnapshots();
  const rootPromise = loadRoot();
  void syncStartupResources();
  const Root = await rootPromise;
  await Promise.all([syncStartupResources(), preloadPromise]);
  return Root;
}

function StartupWindowReveal() {
  React.useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    if (!isStandalone) return;
    void getCurrentWebviewWindow()
      .show()
      .catch((error) => console.error("failed to reveal standalone window", error));
  }, []);

  return null;
}

function renderStartupShell() {
  if (!isStandalone) return;
  reactRoot.render(
    <React.StrictMode>
      <StartupShell />
    </React.StrictMode>,
  );
}

function StartupShell() {
  if (isStandalone) {
    return <StartupWindowReveal />;
  }
  return null;
}

function StartupError() {
  return (
    <main className="app-shell grid h-screen w-screen place-items-center text-sm font-medium text-ink-soft">
      Failed to load Codux.
    </main>
  );
}
