type TerminalTextInputAdapterOptions = {
  textarea: EventTarget;
  isEnabled: () => boolean;
  write: (data: string) => void;
  enabled?: boolean;
};

type PendingTextInput = {
  id: number;
  text: string;
  source: "beforeinput" | "input";
  handledNatively: boolean;
  timer: ReturnType<typeof setTimeout>;
};

export type TerminalTextInputAdapter = {
  noteNativeData: (data: string) => void;
  dispose: () => void;
};

const FALLBACK_COMMIT_DELAY_MS = 16;

export function installTerminalTextInputAdapter({
  textarea,
  isEnabled,
  write,
  enabled = isMacTerminal(),
}: TerminalTextInputAdapterOptions): TerminalTextInputAdapter {
  if (!enabled) {
    return {
      noteNativeData: () => {},
      dispose: () => {},
    };
  }

  let disposed = false;
  let nextPendingId = 1;
  let pendingInputs: PendingTextInput[] = [];
  let lastNativeData = "";
  let lastNativeDataAt = 0;

  const clearPending = () => {
    for (const pending of pendingInputs) {
      clearTimeout(pending.timer);
    }
    pendingInputs = [];
  };

  const recentlyHandledNatively = (text: string) => {
    return lastNativeData.includes(text) && performance.now() - lastNativeDataAt < FALLBACK_COMMIT_DELAY_MS * 2;
  };

  const commitPending = (id: number) => {
    const index = pendingInputs.findIndex((item) => item.id === id);
    if (disposed || index < 0) return;
    const current = pendingInputs[index];
    pendingInputs.splice(index, 1);
    const text = current.text;
    if (!isEnabled()) {
      return;
    }
    if (current.handledNatively || recentlyHandledNatively(text)) {
      return;
    }
    write(text);
  };

  const scheduleFallback = (text: string, source: PendingTextInput["source"]) => {
    if (recentlyHandledNatively(text)) {
      return;
    }
    if (source === "input" && pendingInputs.some((pending) => pending.source === "beforeinput" && pending.text === text)) {
      return;
    }
    const id = nextPendingId;
    nextPendingId += 1;
    const timer = setTimeout(() => commitPending(id), FALLBACK_COMMIT_DELAY_MS);
    pendingInputs.push({ id, text, source, handledNatively: false, timer });
  };

  const handleTextInput = (event: Event) => {
    if (!isEnabled() || !isReliableInputEventText(event)) return;
    const text = (event as InputEvent).data;
    if (!text) return;
    scheduleFallback(text, event.type === "beforeinput" ? "beforeinput" : "input");
  };

  textarea.addEventListener("beforeinput", handleTextInput, true);
  textarea.addEventListener("input", handleTextInput, true);

  return {
    noteNativeData: (data) => {
      if (!data) return;
      lastNativeData = data;
      lastNativeDataAt = performance.now();
      let remaining = data;
      for (const pending of pendingInputs) {
        if (pending.handledNatively) continue;
        const index = remaining.indexOf(pending.text);
        if (index < 0) continue;
        pending.handledNatively = true;
        remaining = remaining.slice(0, index) + remaining.slice(index + pending.text.length);
      }
    },
    dispose: () => {
      disposed = true;
      textarea.removeEventListener("beforeinput", handleTextInput, true);
      textarea.removeEventListener("input", handleTextInput, true);
      clearPending();
    },
  };
}

function isMacTerminal() {
  if (typeof navigator === "undefined") return false;
  return /mac/i.test(navigator.platform);
}

function isReliableInputEventText(event: Event) {
  const inputEvent = event as InputEvent;
  if (inputEvent.inputType !== "insertText" || inputEvent.isComposing) return false;
  const text = inputEvent.data;
  if (typeof text !== "string" || text.length < 1) return false;
  return Array.from(text).every(isPrintableText);
}

function isPrintableText(value: string) {
  if (value.length !== 1) return false;
  const code = value.charCodeAt(0);
  return code >= 0x20 && code !== 0x7f;
}
