import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  defaultSettings,
  flushAppSettings,
  readAppSettings,
  syncAppSettingsFromRust,
  updateAppSettings,
} from "./settings";

const invokeMock = vi.hoisted(() => vi.fn());
const listeners = vi.hoisted(() => new Map<string, Array<(event: { payload: unknown }) => void>>());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = listeners.get(event) ?? [];
    handlers.push(handler);
    listeners.set(event, handlers);
    return Promise.resolve(() => {
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((item) => item !== handler),
      );
    });
  }),
}));

describe("app settings persistence", () => {
  beforeEach(() => {
    vi.resetModules();
    invokeMock.mockReset();
    listeners.clear();
    const store = new Map<string, string>();
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: {},
      localStorage: {
        getItem: (key: string) => store.get(key) ?? null,
        setItem: (key: string, value: string) => store.set(key, value),
        removeItem: (key: string) => store.delete(key),
        clear: () => store.clear(),
      },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    });
    vi.stubGlobal(
      "CustomEvent",
      class {
        detail: unknown;
        constructor(_type: string, init?: { detail?: unknown }) {
          this.detail = init?.detail;
        }
      },
    );
  });

  it("serializes writes and persists the latest remote settings", async () => {
    let releaseFirst: ((value: unknown) => void) | undefined;
    const calls: unknown[] = [];
    invokeMock.mockImplementation((command: string, args: { settings?: typeof defaultSettings } = {}) => {
      if (command !== "app_settings_set") {
        return Promise.resolve(defaultSettings);
      }
      calls.push(args.settings?.remote);
      if (calls.length === 1) {
        return new Promise((resolve) => {
          releaseFirst = resolve;
        });
      }
      return Promise.resolve(args.settings);
    });

    updateAppSettings({
      remote: {
        ...defaultSettings.remote,
        serverURL: "http://10.0.0.2:8088",
      },
    });
    updateAppSettings({
      remote: {
        ...defaultSettings.remote,
        isEnabled: true,
        serverURL: "http://10.0.0.3:8088",
      },
    });

    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({ serverURL: "http://10.0.0.2:8088", isEnabled: false });

    releaseFirst?.({ ...defaultSettings, remote: { ...defaultSettings.remote, serverURL: "http://10.0.0.2:8088" } });
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1]).toMatchObject({ serverURL: "http://10.0.0.3:8088", isEnabled: true });
  });

  it("does not let startup sync overwrite unsaved local settings", async () => {
    let releaseGet: ((value: unknown) => void) | undefined;
    let releaseSet: ((value: unknown) => void) | undefined;
    invokeMock.mockImplementation((command: string, args: { settings?: typeof defaultSettings } = {}) => {
      if (command === "app_settings_get") {
        return new Promise((resolve) => {
          releaseGet = resolve;
        });
      }
      if (command === "app_settings_set") {
        return new Promise((resolve) => {
          releaseSet = () => resolve(args.settings);
        });
      }
      return Promise.resolve(defaultSettings);
    });

    const sync = syncAppSettingsFromRust();
    updateAppSettings({
      remote: {
        ...defaultSettings.remote,
        isEnabled: true,
        serverURL: "http://10.0.0.4:8088",
      },
    });
    releaseGet?.(defaultSettings);
    await sync;

    expect(readAppSettings().remote).toMatchObject({
      isEnabled: true,
      serverURL: "http://10.0.0.4:8088",
    });
    releaseSet?.(undefined);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("app_settings_set", expect.anything()));
  });

  it("flushes pending writes before closing or running remote commands", async () => {
    let releaseFirst: ((value: unknown) => void) | undefined;
    const calls: unknown[] = [];
    invokeMock.mockImplementation((command: string, args: { settings?: typeof defaultSettings } = {}) => {
      if (command !== "app_settings_set") {
        return Promise.resolve(defaultSettings);
      }
      calls.push(args.settings?.remote);
      if (calls.length === 1) {
        return new Promise((resolve) => {
          releaseFirst = resolve;
        });
      }
      return Promise.resolve(args.settings);
    });

    updateAppSettings({
      remote: {
        ...defaultSettings.remote,
        serverURL: "http://10.0.0.5:8088",
      },
    });
    updateAppSettings({
      remote: {
        ...defaultSettings.remote,
        isEnabled: true,
        serverURL: "http://10.0.0.6:8088",
      },
    });

    let flushed = false;
    const flush = flushAppSettings().then((settings) => {
      flushed = true;
      return settings;
    });
    await Promise.resolve();
    expect(flushed).toBe(false);

    releaseFirst?.({ ...defaultSettings, remote: { ...defaultSettings.remote, serverURL: "http://10.0.0.5:8088" } });
    const settings = await flush;
    expect(settings.remote).toMatchObject({ serverURL: "http://10.0.0.6:8088", isEnabled: true });
    expect(calls).toHaveLength(2);
  });

  it("normalizes remote settings without leaking Rust hostId into app settings", async () => {
    invokeMock.mockResolvedValue(defaultSettings);
    updateAppSettings({
      remote: {
        ...defaultSettings.remote,
        hostID: "front-host",
        hostId: "rust-host",
      } as typeof defaultSettings.remote & { hostId: string },
    });

    const remote = readAppSettings().remote as typeof defaultSettings.remote & { hostId?: string };
    expect(remote.hostID).toBe("front-host");
    expect(remote.hostId).toBeUndefined();
  });
});
