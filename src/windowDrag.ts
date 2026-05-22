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

export function startWindowDrag(event: PointerEvent<HTMLElement>) {
  if (!window.__TAURI_INTERNALS__) return;
  if (event.button !== 0) return;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest(INTERACTIVE_SELECTOR)) return;
  const noDragRegion = target?.closest(".no-drag");
  const dragRegion = target?.closest("[data-tauri-drag-region]");
  if (noDragRegion && (!dragRegion || !noDragRegion.contains(dragRegion))) return;
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
