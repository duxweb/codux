import type { PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const INTERACTIVE_SELECTOR = [
  "button",
  "a",
  "input",
  "textarea",
  "select",
  "[role='button']",
  "[contenteditable='true']",
  "[contenteditable='plaintext-only']",
].join(",");

export function isWindowDragTarget(target: EventTarget | null) {
  if (!(target instanceof Element)) {
    return false;
  }
  if (!target.closest("[data-tauri-drag-region]")) {
    return false;
  }
  if (target.closest(INTERACTIVE_SELECTOR)) {
    return false;
  }
  const noDragRegion = target.closest(".no-drag");
  const dragRegion = target.closest("[data-tauri-drag-region]");
  return !(noDragRegion && (!dragRegion || !noDragRegion.contains(dragRegion)));
}

export function startWindowDrag(event: PointerEvent<HTMLElement>) {
  if (!window.__TAURI_INTERNALS__) {
    return;
  }
  if (event.button !== 0) {
    return;
  }
  if (!isWindowDragTarget(event.target)) {
    return;
  }

  event.stopPropagation();
  event.preventDefault();

  const currentWindow = getCurrentWindow();
  if (event.detail >= 2) {
    void currentWindow.toggleMaximize().catch((error) => {
      console.error("failed to toggle window maximize", error);
    });
    return;
  }

  void currentWindow.startDragging().catch((error) => {
    console.error("failed to start window drag", error);
  });
}
