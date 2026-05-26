import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installTerminalTextInputAdapter } from "./inputAdapter";

const FALLBACK_DELAY_MS = 16;

async function flushFallback() {
  await vi.advanceTimersByTimeAsync(FALLBACK_DELAY_MS);
}

function dispatchTextInput(
  textarea: EventTarget,
  data: string,
  options: Partial<InputEventInit> & { type?: "beforeinput" | "input" } = {},
) {
  const event = new Event(options.type ?? "input", {
    bubbles: true,
    cancelable: true,
    composed: true,
  });
  Object.defineProperties(event, {
    data: { value: data },
    inputType: { value: options.inputType ?? "insertText" },
    isComposing: { value: options.isComposing ?? false },
  });
  textarea.dispatchEvent(event);
}

describe("terminal text input adapter", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("does not duplicate text that xterm already emitted natively", async () => {
    const textarea = new EventTarget();
    const write = vi.fn();
    const adapter = installTerminalTextInputAdapter({
      textarea,
      isEnabled: () => true,
      write,
      enabled: true,
    });

    dispatchTextInput(textarea, "?");
    adapter.noteNativeData("?");
    await flushFallback();

    expect(write).not.toHaveBeenCalled();
    adapter.dispose();
  });

  it("writes printable ascii when WebKit input arrives but xterm emits nothing", async () => {
    const textarea = new EventTarget();
    const write = vi.fn();
    const adapter = installTerminalTextInputAdapter({
      textarea,
      isEnabled: () => true,
      write,
      enabled: true,
    });

    dispatchTextInput(textarea, ">");
    await flushFallback();

    expect(write).toHaveBeenCalledWith(">");
    adapter.dispose();
  });

  it("coalesces duplicate beforeinput and input events for the same committed text", async () => {
    const textarea = new EventTarget();
    const write = vi.fn();
    const adapter = installTerminalTextInputAdapter({
      textarea,
      isEnabled: () => true,
      write,
      enabled: true,
    });

    dispatchTextInput(textarea, "？", { type: "beforeinput" });
    dispatchTextInput(textarea, "？");
    await flushFallback();

    expect(write).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith("？");
    adapter.dispose();
  });

  it("drops pending fallback when input is disabled before commit", async () => {
    const textarea = new EventTarget();
    let enabled = true;
    const write = vi.fn();
    const adapter = installTerminalTextInputAdapter({
      textarea,
      isEnabled: () => enabled,
      write,
      enabled: true,
    });

    dispatchTextInput(textarea, "?");
    enabled = false;
    await flushFallback();

    expect(write).not.toHaveBeenCalled();
    adapter.dispose();
  });

  it("keeps text pending until input is enabled before commit", async () => {
    const textarea = new EventTarget();
    let enabled = false;
    const write = vi.fn();
    const adapter = installTerminalTextInputAdapter({
      textarea,
      isEnabled: () => enabled,
      write,
      enabled: true,
    });

    dispatchTextInput(textarea, "?");
    enabled = true;
    await flushFallback();

    expect(write).toHaveBeenCalledWith("?");
    adapter.dispose();
  });

  it("ignores non text input events", async () => {
    const textarea = new EventTarget();
    const write = vi.fn();
    const adapter = installTerminalTextInputAdapter({
      textarea,
      isEnabled: () => true,
      write,
      enabled: true,
    });

    dispatchTextInput(textarea, "x", { inputType: "deleteContentBackward" });
    await flushFallback();

    expect(write).not.toHaveBeenCalled();
    adapter.dispose();
  });
});
