type TerminalInputRegistration = {
  host: HTMLElement;
  textarea: HTMLTextAreaElement;
};

const terminalInputs = new Set<TerminalInputRegistration>();

export function registerTerminalInput(registration: TerminalInputRegistration) {
  terminalInputs.add(registration);

  return () => {
    terminalInputs.delete(registration);
  };
}

export function isTerminalInputTarget(target: EventTarget | null) {
  return isInsideRegisteredTerminal(target) || isTerminalInputElement(target);
}

export function isTerminalInputActive(target?: EventTarget | null) {
  if (isTerminalInputTarget(target ?? null)) return true;
  const active = document.activeElement;
  if (!active) return false;
  return isInsideRegisteredTerminal(active) || isTerminalInputElement(active);
}

function isInsideRegisteredTerminal(target: EventTarget | null) {
  return target instanceof Node && [...terminalInputs].some((item) => item.host.contains(target));
}

function isTerminalInputElement(target: EventTarget | null) {
  const element = target instanceof Element ? target : null;
  return Boolean(element?.closest(".xterm, .xterm-helper-textarea, [data-codux-terminal-input='true']"));
}
