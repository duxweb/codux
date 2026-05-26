import { beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalRuntime } from "./runtime";

const { invokeMock, channels, nextChannelId, MockChannel } = vi.hoisted(() => {
  const invokeMock = vi.fn();
  const channels = new Map<number, unknown>();
  const nextChannelId = { value: 1 };
  class MockChannel<T = unknown> {
    id = nextChannelId.value++;
    onmessage: (value: T) => void = () => {};

    constructor() {
      channels.set(this.id, this);
    }

    toJSON() {
      return this.id;
    }
  }
  return { invokeMock, channels, nextChannelId, MockChannel };
});

vi.mock("@tauri-apps/api/core", () => ({
  Channel: MockChannel,
  invoke: invokeMock,
}));

function channelFromArg<T>(value: unknown) {
  return value as { onmessage: (value: T) => void };
}

describe("terminal runtime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    channels.clear();
    nextChannelId.value = 1;
    invokeMock.mockImplementation((command: string) => {
      if (command === "terminal_create") {
        return Promise.resolve("backend-1");
      }
      if (command === "terminal_snapshot") {
        return Promise.resolve("");
      }
      return Promise.resolve(undefined);
    });
    Object.defineProperty(globalThis, "window", {
      configurable: true,
      value: {
        requestAnimationFrame: (callback: FrameRequestCallback) => setTimeout(callback, 0),
        cancelAnimationFrame: (id: number) => clearTimeout(id),
        setTimeout,
        __TAURI_INTERNALS__: {},
      },
    });
  });

  it("emits state events to subscribers without queueing extra microtasks", () => {
    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "原始标题",
      cwd: "/project",
    });

    const events: string[] = [];
    runtime.subscribe(session.id, (event) => {
      events.push(`${event.type}:${event.type === "closed" ? "" : event.session.title}`);
    });
    events.length = 0;

    runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "新标题",
      cwd: "/project",
    });

    expect(events).toEqual(["state:新标题"]);
  });

  it("starts the backend with raw output and exit channels", async () => {
    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "分屏 1",
      cwd: "/project",
    });

    runtime.ensureStarted(session.id);
    runtime.resize(session.id, 100, 30);
    await vi.runAllTimersAsync();
    await Promise.resolve();
    await Promise.resolve();

    expect(invokeMock).toHaveBeenCalledWith("terminal_create", {
      config: expect.objectContaining({
        projectId: "project-a",
        slotId: "top-1",
        terminalId: session.id,
        sessionKey: "project-a:top-1",
      }),
      onData: expect.any(MockChannel),
      onExit: expect.any(MockChannel),
    });
    expect(runtime.getSession(session.id)?.backendId).toBe("backend-1");
  });

  it("writes input after the backend is attached and output marks the session running", async () => {
    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "分屏 1",
      cwd: "/project",
    });

    runtime.ensureStarted(session.id);
    runtime.resize(session.id, 100, 30);
    await vi.runAllTimersAsync();
    await Promise.resolve();
    await Promise.resolve();

    runtime.write(session.id, "11133333");
    expect(invokeMock).not.toHaveBeenCalledWith("terminal_write", expect.anything());

    const createArgs = invokeMock.mock.calls.find(([command]) => command === "terminal_create")?.[1] as {
      onData: unknown;
    };
    channelFromArg<ArrayBuffer>(createArgs.onData).onmessage(new TextEncoder().encode("➜  project ").buffer);
    expect(runtime.getSession(session.id)?.state).toBe("running");

    runtime.write(session.id, "11133333");
    expect(invokeMock).toHaveBeenCalledWith("terminal_write", {
      sessionId: "backend-1",
      data: "11133333",
    });
  });

  it("uses the full backend snapshot for terminal resets while keeping local replay trimmed", async () => {
    const fullHistory = "x".repeat(100_000);
    invokeMock.mockImplementation((command: string) => {
      if (command === "terminal_create") {
        return Promise.resolve("backend-1");
      }
      if (command === "terminal_snapshot") {
        return Promise.resolve(fullHistory);
      }
      return Promise.resolve(undefined);
    });

    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "分屏 1",
      cwd: "/project",
    });
    const resets: string[] = [];
    runtime.subscribe(session.id, (event) => {
      if (event.type === "reset") resets.push(event.history ?? "");
    });

    runtime.ensureStarted(session.id);
    runtime.resize(session.id, 100, 30);
    await vi.runAllTimersAsync();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(resets[resets.length - 1]).toHaveLength(fullHistory.length);
    expect(runtime.getSession(session.id)?.replayBuffer).toHaveLength(80_000);
    await expect(runtime.snapshot(session.id)).resolves.toBe(fullHistory);
  });

  it("keeps serialized view snapshots and appends detached output deltas", async () => {
    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "分屏 1",
      cwd: "/project",
    });

    runtime.ensureStarted(session.id);
    runtime.resize(session.id, 100, 30);
    await vi.runAllTimersAsync();
    await Promise.resolve();
    await Promise.resolve();

    runtime.saveViewSnapshot(session.id, "serialized-screen");
    const createArgs = invokeMock.mock.calls.find(([command]) => command === "terminal_create")?.[1] as {
      onData: unknown;
    };
    const bytes = new TextEncoder().encode("new output");
    channelFromArg<ArrayBuffer>(createArgs.onData).onmessage(bytes.buffer);

    const snapshot = runtime.takeViewSnapshot(session.id);
    expect(snapshot?.history).toBe("serialized-screen");
    expect(snapshot?.pendingOutputs).toEqual([bytes]);
    expect(runtime.takeViewSnapshot(session.id)).toBeUndefined();
  });

  it("delivers raw bytes and decodes split UTF-8 for replay only", async () => {
    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "分屏 1",
      cwd: "/project",
    });
    const events: unknown[] = [];
    runtime.subscribe(session.id, (event) => {
      if (event.type === "output") events.push(event);
    });

    runtime.ensureStarted(session.id);
    runtime.resize(session.id, 100, 30);
    await vi.runAllTimersAsync();
    await Promise.resolve();
    await Promise.resolve();

    const createArgs = invokeMock.mock.calls.find(([command]) => command === "terminal_create")?.[1] as {
      onData: unknown;
    };
    channelFromArg<ArrayBuffer>(createArgs.onData).onmessage(new Uint8Array([0xe6, 0x8e]).buffer);
    channelFromArg<ArrayBuffer>(createArgs.onData).onmessage(new Uint8Array([0xa8]).buffer);

    expect(events).toHaveLength(2);
    expect(Array.from((events[0] as { bytes: Uint8Array }).bytes)).toEqual([0xe6, 0x8e]);
    expect(Array.from((events[1] as { bytes: Uint8Array }).bytes)).toEqual([0xa8]);
    expect(runtime.getSession(session.id)?.replayBuffer).toBe("推");
  });
});
