import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { installDesktopBrowserBehavior } from "./desktopBehavior";
import { lockRuntimeLocale, syncI18nBundleFromRust } from "./i18n";
import { syncAppSettingsFromRust } from "./settings";
import { initSystemTheme } from "./theme";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

const uninstallDesktopBrowserBehavior = installDesktopBrowserBehavior();

const route = window.location.hash.replace(/^#/, "");
const isStandalone =
  route === "/settings" ||
  route === "/project-create" ||
  route === "/desktop-pet" ||
  route === "/pet-claim" ||
  route === "/pet-dex" ||
  route === "/pet-custom-install" ||
  route === "/memory-manager" ||
  route.startsWith("/terminal") ||
  route.startsWith("/git-diff");
if (isStandalone) {
  document.documentElement.classList.add("standalone-window");
}
if (route === "/desktop-pet") {
  document.documentElement.classList.add("desktop-pet-window");
}
if (route.startsWith("/terminal")) {
  document.documentElement.classList.add("terminal-window");
}

async function loadRoot() {
  if (route === "/settings") {
    const { SettingsWindow } = await import("./windows/SettingsWindow");
    return SettingsWindow;
  }
  if (route === "/project-create") {
    const { ProjectCreateWindow } = await import("./windows/ProjectCreateWindow");
    return ProjectCreateWindow;
  }
  if (route === "/desktop-pet") {
    const { DesktopPetWindow } = await import("./windows/DesktopPetWindow");
    return DesktopPetWindow;
  }
  if (route === "/pet-claim") {
    const { PetClaimWindow } = await import("./windows/PetClaimWindow");
    return PetClaimWindow;
  }
  if (route === "/pet-dex") {
    const { PetDexWindow } = await import("./windows/PetDexWindow");
    return PetDexWindow;
  }
  if (route === "/pet-custom-install") {
    const { PetCustomPetInstallWindow } = await import("./windows/PetCustomPetInstallWindow");
    return PetCustomPetInstallWindow;
  }
  if (route === "/memory-manager") {
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

lockRuntimeLocale();
const uninstallSystemTheme = initSystemTheme();

void loadRoot()
  .then((Root) => {
    const reactRoot = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
    const render = () => {
      reactRoot.render(
        <React.StrictMode>
          {isStandalone && <StandaloneWindowReveal />}
          <Root />
        </React.StrictMode>,
      );
    };

    render();

    void Promise.all([syncAppSettingsFromRust(), syncI18nBundleFromRust()])
      .catch((error) => {
        console.error("failed to bootstrap app state", error);
      })
      .finally(() => {
        lockRuntimeLocale();
        render();
      });
  })
  .catch((error) => {
    console.error("failed to load application", error);
    const root = document.getElementById("root");
    if (root) {
      root.textContent = "Failed to load Codux.";
    }
  });

const uninstallAppRuntime = () => {
  uninstallDesktopBrowserBehavior();
  uninstallSystemTheme();
};

window.addEventListener("beforeunload", uninstallAppRuntime, { once: true });
if (import.meta.hot) {
  import.meta.hot.dispose(uninstallAppRuntime);
}

function StandaloneWindowReveal() {
  React.useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    void getCurrentWebviewWindow()
      .show()
      .catch((error) => console.error("failed to reveal standalone window", error));
  }, []);

  return null;
}
