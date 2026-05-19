import { describeDebugTarget, logTerminalFocusDebug } from "../debug/terminalFocusDebug";

type TerminalInputRegistration = {
  host: HTMLElement;
  textarea: HTMLTextAreaElement;
  blur: () => void;
};

const terminalInputs = new Set<TerminalInputRegistration>();
let isListening = false;
let pointerStartedInsideTerminal = false;
let mouseStartedInsideTerminal = false;

export function registerTerminalInput(registration: TerminalInputRegistration) {
  terminalInputs.add(registration);
  logTerminalFocusDebug("register", {
    host: describeDebugTarget(registration.host),
    textarea: describeDebugTarget(registration.textarea),
    count: terminalInputs.size,
  });
  ensureTerminalFocusListener();

  return () => {
    terminalInputs.delete(registration);
    logTerminalFocusDebug("unregister", {
      host: describeDebugTarget(registration.host),
      count: terminalInputs.size,
    });
    if (terminalInputs.size === 0) {
      teardownTerminalFocusListener();
    }
  };
}

function ensureTerminalFocusListener() {
  if (isListening || typeof window === "undefined") return;
  window.addEventListener("pointerdown", trackTerminalPointerStart, true);
  window.addEventListener("mousedown", trackTerminalMouseStart, true);
  window.addEventListener("pointerup", queueTerminalFocusReleaseFromPointer, false);
  window.addEventListener("click", queueTerminalFocusReleaseFromClick, false);
  window.addEventListener("pointercancel", clearTerminalPointerState, true);
  logTerminalFocusDebug("listener:on");
  isListening = true;
}

function teardownTerminalFocusListener() {
  if (!isListening || typeof window === "undefined") return;
  window.removeEventListener("pointerdown", trackTerminalPointerStart, true);
  window.removeEventListener("mousedown", trackTerminalMouseStart, true);
  window.removeEventListener("pointerup", queueTerminalFocusReleaseFromPointer, false);
  window.removeEventListener("click", queueTerminalFocusReleaseFromClick, false);
  window.removeEventListener("pointercancel", clearTerminalPointerState, true);
  clearTerminalPointerState();
  logTerminalFocusDebug("listener:off");
  isListening = false;
}

function trackTerminalPointerStart(event: PointerEvent) {
  if (event.button !== 0) return;
  pointerStartedInsideTerminal = isInsideRegisteredTerminal(event.target);
}

function trackTerminalMouseStart(event: MouseEvent) {
  if (event.button !== 0) return;
  mouseStartedInsideTerminal = isInsideRegisteredTerminal(event.target);
}

function queueTerminalFocusReleaseFromPointer(event: PointerEvent) {
  if (event.button !== 0) return;
  if (pointerStartedInsideTerminal) return;
  queueTerminalFocusRelease(event.target, event.type);
}

function queueTerminalFocusReleaseFromClick(event: MouseEvent) {
  if (pointerStartedInsideTerminal || mouseStartedInsideTerminal) {
    pointerStartedInsideTerminal = false;
    mouseStartedInsideTerminal = false;
    return;
  }
  queueTerminalFocusRelease(event.target, "click");
}

function queueTerminalFocusRelease(target: EventTarget | null, reason: string) {
  if (!(target instanceof Node)) return;
  const registration = activeTerminalRegistration();
  if (!registration || registration.host.contains(target)) return;

  logTerminalFocusDebug("release-blur", {
    reason,
    target: describeDebugTarget(target),
    activeBefore: describeDebugTarget(document.activeElement),
  });
  releaseTerminalFocus(registration);
  logTerminalFocusDebug("release-blur-done", {
    activeAfter: describeDebugTarget(document.activeElement),
  });
}

function activeTerminalRegistration() {
  const active = document.activeElement;
  return [...terminalInputs].find(
    (item) => active === item.textarea || (active instanceof Node && item.host.contains(active)),
  );
}

function isInsideRegisteredTerminal(target: EventTarget | null) {
  return target instanceof Node && [...terminalInputs].some((item) => item.host.contains(target));
}

function releaseTerminalFocus(registration: TerminalInputRegistration) {
  if (!registration.host.isConnected || !registration.textarea.isConnected) return;
  registration.blur();
  if (document.activeElement === registration.textarea) {
    registration.textarea.blur();
  }
}

function clearTerminalPointerState() {
  pointerStartedInsideTerminal = false;
  mouseStartedInsideTerminal = false;
}
