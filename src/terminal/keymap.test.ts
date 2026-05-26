import { describe, expect, it } from "vitest";
import {
  terminalDeleteSequence,
  terminalLineNavigationSequence,
  terminalModifiedEnterSequence,
  terminalWordNavigationSequence,
  type TerminalKeyEvent,
} from "./keymap";

const event = (partial: Partial<TerminalKeyEvent>): TerminalKeyEvent => ({
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  key: "",
  code: "",
  type: "keydown",
  ...partial,
});

describe("terminal keymap", () => {
  it("maps Option+Arrow to readline word movement", () => {
    expect(terminalWordNavigationSequence(event({ altKey: true, key: "ArrowLeft" }))).toBe("\x1bb");
    expect(terminalWordNavigationSequence(event({ altKey: true, key: "ArrowRight" }))).toBe("\x1bf");
  });

  it("maps Cmd+Arrow to readline line movement on macOS", () => {
    expect(terminalLineNavigationSequence(event({ metaKey: true, key: "ArrowLeft" }), { isMac: true })).toBe("\x01");
    expect(terminalLineNavigationSequence(event({ metaKey: true, key: "ArrowRight" }), { isMac: true })).toBe("\x05");
    expect(terminalLineNavigationSequence(event({ metaKey: true, key: "ArrowLeft" }), { isMac: false })).toBeNull();
  });

  it("maps modified backspace to readline deletion", () => {
    expect(terminalDeleteSequence(event({ metaKey: true, key: "Backspace" }), { isMac: true })).toBe("\x15");
    expect(terminalDeleteSequence(event({ altKey: true, key: "Backspace" }), { isMac: true })).toBe("\x17");
    expect(terminalDeleteSequence(event({ ctrlKey: true, key: "Backspace" }), { isMac: false })).toBe("\x17");
  });

  it("maps Shift+Enter and Cmd+Enter to Meta+Enter on macOS", () => {
    expect(terminalModifiedEnterSequence(event({ shiftKey: true, key: "Enter" }), { isMac: true })).toBe("\x1b\r");
    expect(terminalModifiedEnterSequence(event({ metaKey: true, key: "Enter" }), { isMac: true })).toBe("\x1b\r");
    expect(terminalModifiedEnterSequence(event({ metaKey: true, key: "Enter" }), { isMac: false })).toBeNull();
  });
});
