const TEXT_ENTRY_SELECTOR_ITEMS = [
  "input",
  "textarea",
  "select",
  "[contenteditable='true']",
  "[contenteditable='plaintext-only']",
  ".cm-editor",
  ".xterm",
  ".xterm-helper-textarea",
  "[data-allow-tab-navigation]",
];

const KEYBOARD_MANAGED_SELECTOR_ITEMS = [
  ...TEXT_ENTRY_SELECTOR_ITEMS,
  "[role='tab']",
  "[role='tablist']",
  "[role='menu']",
  "[role='menuitem']",
  "[role='menuitemcheckbox']",
  "[role='menuitemradio']",
  "[role='listbox']",
  "[role='option']",
  "[role='combobox']",
  "[role='dialog']",
  "[aria-haspopup='menu']",
  "[aria-haspopup='listbox']",
  "[data-slot='tabs']",
  "[data-slot='tab-list']",
  "[data-slot='dropdown-menu']",
  "[data-slot='list-box']",
  "[data-slot='popover']",
];

const SHORTCUT_BOUNDARY_SELECTOR_ITEMS = [
  "[role='dialog']",
  "[role='menu']",
  "[role='menuitem']",
  "[role='menuitemcheckbox']",
  "[role='menuitemradio']",
  "[role='listbox']",
  "[role='option']",
  "[role='combobox']",
  "[aria-haspopup='menu']",
  "[aria-haspopup='listbox']",
  "[data-slot='dropdown-menu']",
  "[data-slot='list-box']",
  "[data-slot='popover']",
];

export const TEXT_ENTRY_SELECTOR = TEXT_ENTRY_SELECTOR_ITEMS.join(", ");
export const KEYBOARD_MANAGED_SELECTOR = KEYBOARD_MANAGED_SELECTOR_ITEMS.join(", ");
export const SHORTCUT_BOUNDARY_SELECTOR = SHORTCUT_BOUNDARY_SELECTOR_ITEMS.join(", ");

export function closestElement(target: EventTarget | null, selector: string) {
  return target instanceof Element ? target.closest(selector) : null;
}

export function isTextEntryTarget(target: EventTarget | null) {
  return Boolean(closestElement(target, TEXT_ENTRY_SELECTOR));
}

export function isKeyboardManagedTarget(target: EventTarget | null) {
  return Boolean(closestElement(target, KEYBOARD_MANAGED_SELECTOR));
}

export function isShortcutBoundaryTarget(target: EventTarget | null) {
  return Boolean(closestElement(target, SHORTCUT_BOUNDARY_SELECTOR));
}

export function isOverlayNavigationKey(event: KeyboardEvent) {
  return (
    event.key === "ArrowDown" ||
    event.key === "ArrowUp" ||
    event.key === "ArrowLeft" ||
    event.key === "ArrowRight" ||
    event.key === "Home" ||
    event.key === "End" ||
    event.key === "PageDown" ||
    event.key === "PageUp" ||
    event.key === "Enter" ||
    event.key === " " ||
    event.key === "Escape"
  );
}

export function isShortcutBoundaryEvent(event: KeyboardEvent) {
  if (!isOverlayNavigationKey(event)) return false;
  return isShortcutBoundaryTarget(event.target) || isShortcutBoundaryTarget(document.activeElement);
}
