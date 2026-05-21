const ALLOW_NATIVE_CONTEXT_MENU_SELECTOR = "[data-native-context-menu]";
const ALLOW_BROWSER_DRAG_SELECTOR = "[data-browser-drag], [data-app-draggable]";
const DROP_ZONE_SELECTOR = "[data-drop-zone], [data-allow-drop]";
const TEXT_ENTRY_SELECTOR = [
  "input",
  "textarea",
  "select",
  "[contenteditable='true']",
  "[contenteditable='plaintext-only']",
  ".cm-editor",
  ".xterm",
  "[data-allow-tab-navigation]",
].join(", ");
const FOCUSABLE_CHROME_SELECTOR = ["button", "a[href]", "[role='button']", "[tabindex]"].join(", ");

export function installDesktopBrowserBehavior() {
  if (!window.__TAURI_INTERNALS__) {
    return () => {};
  }

  const controller = new AbortController();
  const options: AddEventListenerOptions = {
    capture: true,
    signal: controller.signal,
  };

  window.addEventListener("contextmenu", preventNativeContextMenu, options);
  window.addEventListener("dragstart", preventBrowserDragStart, options);
  window.addEventListener("dragenter", preventBrowserFileDrop, options);
  window.addEventListener("dragover", preventBrowserFileDrop, options);
  window.addEventListener("drop", preventBrowserFileDrop, options);
  window.addEventListener("pointerup", clearChromeFocusAfterPointer, options);
  window.addEventListener("keydown", preventDesktopKeyboardDefaults, options);

  return () => controller.abort();
}

function preventNativeContextMenu(event: MouseEvent) {
  if (closestElement(event.target, ALLOW_NATIVE_CONTEXT_MENU_SELECTOR)) {
    return;
  }
  if (closestElement(event.target, TEXT_ENTRY_SELECTOR)) {
    return;
  }
  event.preventDefault();
}

function preventBrowserDragStart(event: DragEvent) {
  if (closestElement(event.target, ALLOW_BROWSER_DRAG_SELECTOR)) {
    return;
  }
  event.preventDefault();
}

function preventBrowserFileDrop(event: DragEvent) {
  if (closestElement(event.target, DROP_ZONE_SELECTOR)) {
    return;
  }
  event.preventDefault();
}

function preventDesktopKeyboardDefaults(event: KeyboardEvent) {
  if (closestElement(event.target, TEXT_ENTRY_SELECTOR)) {
    return;
  }

  if (event.key === "Tab" && !closestElement(event.target, TEXT_ENTRY_SELECTOR)) {
    event.preventDefault();
    return;
  }

  const active = document.activeElement;
  if (active instanceof HTMLButtonElement && event.target === active && isPlainPrintableKey(event)) {
    event.preventDefault();
    active.blur();
  }
}

function clearChromeFocusAfterPointer(event: PointerEvent) {
  const target = event.target;
  if (closestElement(target, TEXT_ENTRY_SELECTOR)) {
    return;
  }
  const active = document.activeElement;
  if (!(active instanceof HTMLElement)) {
    return;
  }
  if (active === document.body || closestElement(active, TEXT_ENTRY_SELECTOR)) {
    return;
  }
  if (
    active.matches(FOCUSABLE_CHROME_SELECTOR) &&
    (active === target || active.contains(target instanceof Node ? target : null))
  ) {
    window.requestAnimationFrame(() => {
      if (document.activeElement === active) {
        active.blur();
      }
    });
  }
}

function closestElement(target: EventTarget | null, selector: string) {
  return target instanceof Element ? target.closest(selector) : null;
}

function isPlainPrintableKey(event: KeyboardEvent) {
  return event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey;
}
