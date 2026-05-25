import { beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalRuntime } from "./runtime";
import type { TerminalEvent } from "../types";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

describe("terminal runtime", () => {
  let eventHandler: ((event: { payload: TerminalEvent }) => void) | undefined;

  beforeEach(() => {
    vi.useFakeTimers();
    eventHandler = undefined;
    invokeMock.mockReset();
    listenMock.mockReset();
    listenMock.mockImplementation((_eventName: string, handler: typeof eventHandler) => {
      eventHandler = handler;
      return Promise.resolve(() => undefined);
    });
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
    Object.defineProperty(globalThis.window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
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

  it("does not write input until the backend has produced initial output", async () => {
    const runtime = new TerminalRuntime();
    const session = runtime.ensureTerminal({
      projectId: "project-a",
      slotId: "top-1",
      title: "分屏 1",
      cwd: "/project",
    });

    expect(session.projectId).toBe("project-a");
    expect(session.slotId).toBe("top-1");
    expect(session.id).toMatch(/^term-/);

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
    });
    expect(runtime.getSession(session.id)?.backendId).toBe("backend-1");
    expect(runtime.getSession(session.id)?.state).toBe("starting");

    runtime.write(session.id, "11133333");
    expect(invokeMock).not.toHaveBeenCalledWith("terminal_write", expect.anything());

    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        text: "➜  project git:(main) ",
      },
    });
    expect(runtime.getSession(session.id)?.state).toBe("running");

    runtime.write(session.id, "11133333");
    expect(invokeMock).toHaveBeenCalledWith("terminal_write", {
      sessionId: "backend-1",
      data: "11133333",
    });
  });

  it("hydrates output that arrived before the backend id was registered", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "terminal_create") {
        eventHandler?.({
          payload: {
            kind: "output",
            sessionId: "backend-early",
            text: "early prompt",
          },
        });
        return Promise.resolve("backend-early");
      }
      if (command === "terminal_snapshot") {
        return Promise.resolve("early prompt");
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

    runtime.ensureStarted(session.id);
    runtime.resize(session.id, 100, 30);
    await vi.runAllTimersAsync();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(runtime.getSession(session.id)?.backendId).toBe("backend-early");
    expect(runtime.getSession(session.id)?.replayBuffer).toBe("early prompt");
    expect(runtime.getSession(session.id)?.state).toBe("running");
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
    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        text: "new output",
      },
    });

    const snapshot = runtime.takeViewSnapshot(session.id);
    expect(snapshot?.history).toBe("serialized-screen");
    expect(snapshot?.pendingOutputs).toEqual(["new output"]);
    expect(runtime.takeViewSnapshot(session.id)).toBeUndefined();
  });

  it("does not include terminal history in output events", async () => {
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

    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        text: "line 1",
      },
    });
    await vi.runOnlyPendingTimersAsync();
    await Promise.resolve();

    expect(events).toHaveLength(1);
    expect((events[0] as { session: { history?: string } }).session.history).toBeUndefined();
    expect(runtime.getSession(session.id)?.replayBuffer).toBe("line 1");
  });

  it("keeps raw terminal bytes on output events when the backend provides them", async () => {
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

    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        text: "推",
        bytesBase64: "5o6o",
      },
    });
    await vi.runOnlyPendingTimersAsync();
    await Promise.resolve();

    expect(events).toHaveLength(1);
    expect(Array.from((events[0] as { bytes: Uint8Array }).bytes)).toEqual([0xe6, 0x8e, 0xa8]);
    expect(runtime.getSession(session.id)?.replayBuffer).toBe("推");
  });

  it("delivers raw bytes even when a UTF-8 character is split across backend chunks", async () => {
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

    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        text: "",
        bytesBase64: "5o4=",
      },
    });
    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        text: "推",
        bytesBase64: "qA==",
      },
    });
    await vi.runOnlyPendingTimersAsync();
    await Promise.resolve();

    expect(events).toHaveLength(1);
    expect(Array.from((events[0] as { bytes: Uint8Array }).bytes)).toEqual([0xe6, 0x8e, 0xa8]);
    expect(runtime.getSession(session.id)?.replayBuffer).toBe("推");
  });

  it("keeps byte-only backend output out of the frontend replay buffer", async () => {
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

    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        bytesBase64: "5o4=",
      },
    });
    eventHandler?.({
      payload: {
        kind: "output",
        sessionId: "backend-1",
        bytesBase64: "qA==",
      },
    });
    await vi.runOnlyPendingTimersAsync();
    await Promise.resolve();

    expect(events).toHaveLength(1);
    expect(Array.from((events[0] as { bytes: Uint8Array }).bytes)).toEqual([0xe6, 0x8e, 0xa8]);
    expect(runtime.getSession(session.id)?.replayBuffer).toBe("");
  });
});
