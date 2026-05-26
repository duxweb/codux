export type TerminalKeyEvent = Pick<
  KeyboardEvent,
  "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "key" | "code" | "type"
>;

export type TerminalKeymapOptions = {
  isMac: boolean;
};

export function terminalWordNavigationSequence(event: TerminalKeyEvent) {
  if (!event.altKey || event.ctrlKey || event.metaKey) return null;
  if (isKey(event, "ArrowLeft")) return "\x1bb";
  if (isKey(event, "ArrowRight")) return "\x1bf";
  return null;
}

export function terminalLineNavigationSequence(event: TerminalKeyEvent, options: TerminalKeymapOptions) {
  if (!options.isMac) return null;
  if (!event.metaKey || event.altKey || event.ctrlKey) return null;
  if (isKey(event, "ArrowLeft")) return "\x01";
  if (isKey(event, "ArrowRight")) return "\x05";
  return null;
}

export function terminalDeleteSequence(event: TerminalKeyEvent, options: TerminalKeymapOptions) {
  if (!isKey(event, "Backspace")) return null;
  if (options.isMac) {
    if (event.metaKey && !event.altKey && !event.ctrlKey) return "\x15";
    if (event.altKey && !event.metaKey && !event.ctrlKey) return "\x17";
    return null;
  }
  if (event.ctrlKey && !event.altKey && !event.metaKey) return "\x17";
  return null;
}

export function terminalModifiedEnterSequence(event: TerminalKeyEvent, options: TerminalKeymapOptions) {
  if (!isKey(event, "Enter")) return null;
  if (event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey) return "\x1b\r";
  if (options.isMac && event.metaKey && !event.altKey && !event.ctrlKey && !event.shiftKey) return "\x1b\r";
  return null;
}

function isKey(event: TerminalKeyEvent, key: string) {
  return event.key === key || event.code === key;
}
