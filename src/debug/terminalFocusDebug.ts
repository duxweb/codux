const DEBUG_KEY = "coduxTerminalFocusDebug";

export function terminalFocusDebugEnabled() {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(DEBUG_KEY) === "1";
  } catch {
    return false;
  }
}

export function logTerminalFocusDebug(message: string, data?: Record<string, unknown>) {
  if (!terminalFocusDebugEnabled()) return;
  const timestamp = new Date().toISOString().slice(11, 23);
  console.log(`[terminal-focus ${timestamp}] ${message}`, data ?? {});
}

export function describeDebugTarget(target: EventTarget | null) {
  if (!(target instanceof Element)) return String(target);
  const parts = [target.tagName.toLowerCase()];
  const aria = target.getAttribute("aria-label");
  const role = target.getAttribute("role");
  const className = typeof target.className === "string" ? target.className : "";
  if (target.id) parts.push(`#${target.id}`);
  if (role) parts.push(`[role=${role}]`);
  if (aria) parts.push(`[aria=${aria}]`);
  if (className) parts.push(`.${className.split(/\s+/).filter(Boolean).slice(0, 4).join(".")}`);
  return parts.join("");
}

export function installTerminalFocusEventTrace() {
  if (typeof window === "undefined" || !terminalFocusDebugEnabled()) return () => undefined;

  const events = ["pointerdown", "pointerup", "mousedown", "mouseup", "click", "focusin", "focusout"];
  const listener = (event: Event) => {
    logTerminalFocusDebug(`dom:${event.type}`, {
      target: describeDebugTarget(event.target),
      active: describeDebugTarget(document.activeElement),
      phase: event.eventPhase,
      defaultPrevented: event.defaultPrevented,
      isTrusted: "isTrusted" in event ? event.isTrusted : undefined,
    });
  };

  for (const eventName of events) {
    window.addEventListener(eventName, listener, true);
  }

  return () => {
    for (const eventName of events) {
      window.removeEventListener(eventName, listener, true);
    }
  };
}
