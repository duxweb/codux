import type { PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const NO_DRAG_SELECTOR = [
  ".no-drag",
  "button",
  "a",
  "input",
  "textarea",
  "select",
  "[role='button']",
  "[contenteditable='true']",
  "[contenteditable='plaintext-only']",
].join(",");

export function startWindowDrag(event: PointerEvent<HTMLElement>) {
  if (!window.__TAURI_INTERNALS__) return;
  if (event.button !== 0) return;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest(NO_DRAG_SELECTOR)) return;
  event.preventDefault();

  const currentWindow = getCurrentWindow();
  if (event.detail >= 2) {
    void currentWindow.toggleMaximize().catch((error) => {
      console.error("failed to toggle window maximize", error);
    });
    return;
  }

  void currentWindow
    .startDragging()
    .catch((error) => {
      console.error("failed to start window drag", error);
    });
}
