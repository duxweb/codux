export type TerminalKeyEvent = Pick<
  KeyboardEvent,
  "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "key" | "code" | "type" | "isComposing" | "keyCode"
>;

function isComposing(event: TerminalKeyEvent) {
  return event.isComposing || event.keyCode === 229;
}

export function terminalControlSequence(event: TerminalKeyEvent): string | null {
  if (event.type !== "keydown" || isComposing(event)) return null;

  if (isShiftEnter(event)) return "\x1b[13;2u";
  if (isLineNavigation(event, "left")) return "\x01";
  if (isLineNavigation(event, "right")) return "\x05";
  if (isWordNavigation(event, "left")) return "\x1bb";
  if (isWordNavigation(event, "right")) return "\x1bf";

  return null;
}

function isShiftEnter(event: TerminalKeyEvent) {
  return isKey(event, "Enter") && event.shiftKey && !event.metaKey && !event.altKey && !event.ctrlKey;
}

function isLineNavigation(event: TerminalKeyEvent, direction: "left" | "right") {
  return (
    event.metaKey &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    isKey(event, direction === "left" ? "ArrowLeft" : "ArrowRight")
  );
}

function isWordNavigation(event: TerminalKeyEvent, direction: "left" | "right") {
  return (
    event.altKey &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    isKey(event, direction === "left" ? "ArrowLeft" : "ArrowRight")
  );
}

function isKey(event: TerminalKeyEvent, key: string) {
  return event.key === key || event.code === key;
}
