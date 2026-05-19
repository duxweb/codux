import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type AppSettings = {
  language: string;
  shell: string;
  showsDockBadge: boolean;
  pet: PetSettings;
  ai: AISettings;
  sleepMode: string;
  gitRefresh: string;
  aiRefresh: string;
  aiBackgroundRefresh: string;
  statisticsMode: string;
  theme: string;
  background: string;
  terminalFontSize: string;
  iconStyle: string;
  notificationChannels: Record<string, NotificationChannelSettings>;
  shortcuts: Record<string, string>;
  update: UpdateSettings;
  remote: RemoteSettings;
  developerHud: boolean;
  developerRefresh: string;
};

export type NotificationChannelSettings = {
  enabled: boolean;
  endpoint: string;
  token: string;
};

export type UpdateSettings = {
  enabled: boolean;
  channel: string;
  endpoint: string;
};

export type RemoteSettings = {
  enabled: boolean;
  relayUrl: string;
};

export type PetSettings = {
  enabled: boolean;
  desktopWidget: boolean;
  staticMode: boolean;
  reminders: boolean;
  /** Legacy Tauri settings kept only for one-time migration into ai.pet. */
  speechMode: string;
  /** Legacy Tauri settings kept only for one-time migration into ai.pet. */
  speechFrequency: string;
  desktopScale: string;
};

export type AISettings = {
  globalPrompt: string;
  memory: AIMemorySettings;
  pet: AIPetSettings;
  providers: AIProviderSettings[];
};

export type AIMemorySettings = {
  enabled: boolean;
  automaticInjectionEnabled: boolean;
  automaticExtractionEnabled: boolean;
  allowCrossProjectUserRecall: boolean;
  defaultExtractorProviderId: string;
  maxInjectedUserWorkingMemories: number;
  maxInjectedProjectWorkingMemories: number;
  maxActiveWorkingEntries: number;
  maxSummaryVersions: number;
  summaryTargetTokenBudget: number;
  maxInjectedSummaryTokens: number;
  extractionIdleDelaySeconds: number;
  sessionExtractionCooldownSeconds: number;
  maxExtractionTranscriptLines: number;
  maxExtractionTranscriptTokens: number;
};

export type AIPetSettings = {
  speechMode: string;
  speechFrequency: string;
  speechLlmEnabled: boolean;
  speechProviderId: string;
  speechQuietDuringWork: boolean;
  speechLouderAtNight: boolean;
  speechMuteOnFullscreen: boolean;
  speechQuietHoursStart: number | null;
  speechQuietHoursEnd: number | null;
  speechTemporaryMuteUntil: number | null;
};

export type AIProviderSettings = {
  id: string;
  kind: "openAICompatible" | "anthropic" | "localLlama";
  displayName: string;
  isEnabled: boolean;
  model: string;
  baseUrl: string;
  apiKey: string;
  useForMemoryExtraction: boolean;
  priority: number;
};

const SETTINGS_KEY = "codux:settings:v1";
const DEFAULT_UPDATE_ENDPOINT = "https://github.com/duxweb/codux/releases/download/tauri-stable/latest.json";
const LEGACY_UPDATE_ENDPOINTS = new Set([
  "https://github.com/duxweb/codux/releases/latest/download/codux-tauri-latest.json",
  "https://github.com/duxweb/codux/releases/latest/download/latest.json",
]);
let cachedSettings: AppSettings | null = null;
let settingsSyncPromise: Promise<AppSettings> | null = null;
let settingsListenerInstalled = false;

export const defaultSettings: AppSettings = {
  language: "system",
  shell: "system",
  showsDockBadge: true,
  pet: {
    enabled: true,
    desktopWidget: false,
    staticMode: false,
    reminders: false,
    speechMode: "mixed",
    speechFrequency: "normal",
    desktopScale: "1",
  },
  ai: {
    globalPrompt: "",
    memory: {
      enabled: true,
      automaticInjectionEnabled: true,
      automaticExtractionEnabled: true,
      allowCrossProjectUserRecall: true,
      defaultExtractorProviderId: "automatic",
      maxInjectedUserWorkingMemories: 4,
      maxInjectedProjectWorkingMemories: 6,
      maxActiveWorkingEntries: 50,
      maxSummaryVersions: 10,
      summaryTargetTokenBudget: 900,
      maxInjectedSummaryTokens: 900,
      extractionIdleDelaySeconds: 120,
      sessionExtractionCooldownSeconds: 900,
      maxExtractionTranscriptLines: 80,
      maxExtractionTranscriptTokens: 8000,
    },
    pet: {
      speechMode: "off",
      speechFrequency: "normal",
      speechLlmEnabled: false,
      speechProviderId: "automatic",
      speechQuietDuringWork: true,
      speechLouderAtNight: false,
      speechMuteOnFullscreen: true,
      speechQuietHoursStart: null,
      speechQuietHoursEnd: null,
      speechTemporaryMuteUntil: null,
    },
    providers: [],
  },
  sleepMode: "off",
  gitRefresh: "60",
  aiRefresh: "180",
  aiBackgroundRefresh: "600",
  statisticsMode: "normalized",
  theme: "Auto",
  background: "Auto",
  terminalFontSize: "14",
  iconStyle: "default",
  notificationChannels: {},
  shortcuts: {},
  update: {
    enabled: true,
    channel: "stable",
    endpoint: DEFAULT_UPDATE_ENDPOINT,
  },
  remote: {
    enabled: false,
    relayUrl: "",
  },
  developerHud: false,
  developerRefresh: "3",
};

export function readAppSettings(): AppSettings {
  if (cachedSettings) return cachedSettings;
  if (typeof window === "undefined") return defaultSettings;
  try {
    const raw = window.localStorage.getItem(SETTINGS_KEY);
    if (!raw) return defaultSettings;
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    cachedSettings = normalizeAppSettings({
      ...defaultSettings,
      ...parsed,
      notificationChannels: {
        ...defaultSettings.notificationChannels,
        ...(parsed.notificationChannels ?? {}),
      },
      shortcuts: {
        ...defaultSettings.shortcuts,
        ...(parsed.shortcuts ?? {}),
      },
      update: {
        ...defaultSettings.update,
        ...(parsed.update ?? {}),
      },
      remote: {
        ...defaultSettings.remote,
        ...(parsed.remote ?? {}),
      },
      pet: {
        ...defaultSettings.pet,
        ...(parsed.pet ?? {}),
      },
      ai: normalizeAISettings(parsed.ai, parsed.pet),
    });
    return cachedSettings;
  } catch {
    return defaultSettings;
  }
}

export function writeAppSettings(next: AppSettings) {
  cachedSettings = normalizeAppSettings(next);
  window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(cachedSettings));
  if (window.__TAURI_INTERNALS__) {
    void invoke<AppSettings>("app_settings_set", { settings: cachedSettings }).catch((error) => {
      console.error("failed to persist app settings", error);
    });
  }
}

export function updateAppSettings(patch: Partial<AppSettings>) {
  const next = {
    ...readAppSettings(),
    ...patch,
  };
  writeAppSettings(next);
  window.dispatchEvent(new CustomEvent("codux:settings-changed", { detail: next }));
  return next;
}

export function subscribeAppSettings(listener: (settings: AppSettings) => void) {
  const handle = (event: Event) => {
    const detail = event instanceof CustomEvent ? event.detail : null;
    listener(detail ?? readAppSettings());
  };
  window.addEventListener("codux:settings-changed", handle);
  return () => window.removeEventListener("codux:settings-changed", handle);
}

export async function syncAppSettingsFromRust() {
  if (!window.__TAURI_INTERNALS__) return readAppSettings();
  settingsSyncPromise ??= invoke<AppSettings>("app_settings_get")
    .then((settings) => {
      cachedSettings = normalizeAppSettings(settings);
      window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(cachedSettings));
      window.dispatchEvent(new CustomEvent("codux:settings-changed", { detail: cachedSettings }));
      return cachedSettings;
    })
    .catch((error) => {
      console.error("failed to load app settings", error);
      return readAppSettings();
    })
    .finally(() => {
      settingsSyncPromise = null;
    });
  installSettingsEventBridge();
  return settingsSyncPromise;
}

function installSettingsEventBridge() {
  if (!window.__TAURI_INTERNALS__ || settingsListenerInstalled) return;
  settingsListenerInstalled = true;
  void listen<AppSettings>("settings:updated", (event) => {
    cachedSettings = normalizeAppSettings(event.payload);
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(cachedSettings));
    window.dispatchEvent(new CustomEvent("codux:settings-changed", { detail: cachedSettings }));
  }).catch((error) => {
    console.error("failed to listen for settings updates", error);
    settingsListenerInstalled = false;
  });
}

function normalizeAppSettings(settings: Partial<AppSettings>): AppSettings {
  const update = {
    ...defaultSettings.update,
    ...(settings.update ?? {}),
  };
  update.endpoint = normalizeUpdateEndpoint(update.endpoint, update.enabled);
  return {
    ...defaultSettings,
    ...settings,
    notificationChannels: {
      ...defaultSettings.notificationChannels,
      ...(settings.notificationChannels ?? {}),
    },
    shortcuts: {
      ...defaultSettings.shortcuts,
      ...(settings.shortcuts ?? {}),
    },
    update: {
      ...update,
    },
    remote: {
      ...defaultSettings.remote,
      ...(settings.remote ?? {}),
    },
    pet: {
      ...defaultSettings.pet,
      ...(settings.pet ?? {}),
    },
    ai: normalizeAISettings(settings.ai, settings.pet),
  };
}

function normalizeUpdateEndpoint(endpoint: string, enabled: boolean) {
  const trimmed = endpoint.trim();
  if (!enabled) return trimmed;
  if (!trimmed || LEGACY_UPDATE_ENDPOINTS.has(trimmed)) return DEFAULT_UPDATE_ENDPOINT;
  return trimmed;
}

function normalizeAISettings(settings?: Partial<AISettings>, legacyPet?: Partial<PetSettings>): AISettings {
  const rawPet: Partial<AIPetSettings> = settings?.pet ?? {};
  const legacySpeechMode =
    rawPet.speechMode === undefined && typeof legacyPet?.speechMode === "string"
      ? legacyPet.speechMode
      : undefined;
  const legacySpeechFrequency =
    rawPet.speechFrequency === undefined && typeof legacyPet?.speechFrequency === "string"
      ? legacyPet.speechFrequency
      : undefined;
  return {
    ...defaultSettings.ai,
    ...(settings ?? {}),
    memory: {
      ...defaultSettings.ai.memory,
      ...(settings?.memory ?? {}),
    },
    pet: {
      ...defaultSettings.ai.pet,
      ...rawPet,
      ...(legacySpeechMode !== undefined ? { speechMode: legacySpeechMode } : {}),
      ...(legacySpeechFrequency !== undefined ? { speechFrequency: legacySpeechFrequency } : {}),
      speechQuietHoursStart: normalizeOptionalHour(rawPet.speechQuietHoursStart),
      speechQuietHoursEnd: normalizeOptionalHour(rawPet.speechQuietHoursEnd),
      speechTemporaryMuteUntil: normalizeOptionalTimestamp(rawPet.speechTemporaryMuteUntil),
    },
    providers: (settings?.providers ?? []).map((provider) => ({
      id: provider.id,
      kind: provider.kind,
      displayName: provider.displayName,
      isEnabled: provider.isEnabled,
      model: provider.model,
      baseUrl: provider.baseUrl,
      apiKey: provider.apiKey,
      useForMemoryExtraction: provider.useForMemoryExtraction,
      priority: provider.priority,
    })),
  };
}

function normalizeOptionalHour(value: unknown): number | null {
  if (value === null || value === undefined || value === "") return null;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return null;
  return Math.max(0, Math.min(23, Math.round(parsed)));
}

function normalizeOptionalTimestamp(value: unknown): number | null {
  if (value === null || value === undefined || value === "") return null;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) return null;
  return Math.round(parsed);
}

export function readTerminalFontSize(settings = readAppSettings()) {
  const parsed = Number(settings.terminalFontSize);
  if (!Number.isFinite(parsed)) return 14;
  return Math.max(10, Math.min(28, Math.round(parsed)));
}

export function readConfiguredShell(settings = readAppSettings()) {
  const value = settings.shell.trim();
  if (!value || value === "system") return undefined;
  return value;
}
