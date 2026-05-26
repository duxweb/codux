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
  themeColor: string;
  terminalFontSize: string;
  iconStyle: string;
  notificationChannels: Record<string, NotificationChannelSettings>;
  shortcuts: Record<string, string>;
  update: UpdateSettings;
  remote: RemoteSettings;
  developerHud: boolean;
  developerRefresh: string;
};

export type AIStatisticsMode = "normalized" | "includingCache";

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
  isEnabled: boolean;
  serverURL: string;
  hostID: string;
  hostToken: string;
  hostPrivateKey: string;
  hostPublicKey: string;
  cachedDevices: RemoteDeviceSettings[];
};

export type RemoteDeviceSettings = {
  id: string;
  hostId: string;
  name: string;
  publicKey: string;
  createdAt: string;
  lastSeen: string;
  revokedAt?: string | null;
  online?: boolean | null;
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
};

export type AISettings = {
  globalPrompt: string;
  gitCommitMessageProviderId: string;
  gitCommitMessageTone: string;
  gitCommitMessageLanguage: string;
  gitCommitMessageStyleRules: string;
  runtimeTools: AIRuntimeToolSettings;
  memory: AIMemorySettings;
  pet: AIPetSettings;
  providers: AIProviderSettings[];
};

export type AIToolPermissionMode = "default" | "fullAccess";

export type AICodexReasoningEffort = "none" | "minimal" | "low" | "medium" | "high" | "xhigh";

export type AIRuntimeToolSettings = {
  codex: AIToolPermissionMode;
  claudeCode: AIToolPermissionMode;
  gemini: AIToolPermissionMode;
  opencode: AIToolPermissionMode;
  kiro: AIToolPermissionMode;
  codexModel: string;
  claudeCodeModel: string;
  geminiModel: string;
  opencodeModel: string;
  kiroModel: string;
  codexEffort: AICodexReasoningEffort;
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
  maxIndexSessions: number;
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
  kind:
    | "openai"
    | "openAICompatible"
    | "anthropic"
    | "deepseek"
    | "gemini"
    | "groq"
    | "openrouter"
    | "ollama"
    | "localLlama";
  displayName: string;
  isEnabled: boolean;
  model: string;
  baseUrl: string;
  apiKey: string;
  useForMemoryExtraction: boolean;
  priority: number;
};

const SETTINGS_KEY = "codux:settings:v1";
export const UPDATE_CHANNEL_ENDPOINTS: Record<"stable" | "beta", string> = {
  stable: "https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json",
  beta: "https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json",
};
const APP_VERSION = "1.0.5";
const DEFAULT_UPDATE_CHANNEL: "stable" | "beta" = APP_VERSION.includes("-") ? "beta" : "stable";
const DEFAULT_UPDATE_ENDPOINT = UPDATE_CHANNEL_ENDPOINTS[DEFAULT_UPDATE_CHANNEL];
const LEGACY_UPDATE_ENDPOINTS = new Set([
  "https://github.com/duxweb/codux/releases/latest/download/codux-tauri-latest.json",
  "https://github.com/duxweb/codux/releases/latest/download/latest.json",
  "https://github.com/duxweb/codux/releases/download/tauri-stable/latest.json",
  "https://github.com/duxweb/codux/releases/download/tauri-beta/latest.json",
]);
let cachedSettings: AppSettings | null = null;
let settingsSyncPromise: Promise<AppSettings> | null = null;
let settingsListenerInstalled = false;
let settingsWriteSequence = 0;
let persistedSettingsSequence = 0;
let settingsWriteInFlight = false;
let settingsFlushWaiters: Array<{
  resolve: (settings: AppSettings) => void;
  reject: (error: unknown) => void;
}> = [];

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
  },
  ai: {
    globalPrompt: "",
    gitCommitMessageProviderId: "automatic",
    gitCommitMessageTone: "conventional",
    gitCommitMessageLanguage: "application",
    gitCommitMessageStyleRules: "",
    runtimeTools: {
      codex: "default",
      claudeCode: "default",
      gemini: "default",
      opencode: "default",
      kiro: "default",
      codexModel: "",
      claudeCodeModel: "",
      geminiModel: "",
      opencodeModel: "",
      kiroModel: "",
      codexEffort: "medium",
    },
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
      extractionIdleDelaySeconds: 300,
      sessionExtractionCooldownSeconds: 900,
      maxIndexSessions: 20,
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
  themeColor: "Blue",
  terminalFontSize: "14",
  iconStyle: "default",
  notificationChannels: {},
  shortcuts: {},
  update: {
    enabled: true,
    channel: DEFAULT_UPDATE_CHANNEL,
    endpoint: DEFAULT_UPDATE_ENDPOINT,
  },
  remote: {
    isEnabled: false,
    serverURL: "http://127.0.0.1:8088",
    hostID: "",
    hostToken: "",
    hostPrivateKey: "",
    hostPublicKey: "",
    cachedDevices: [],
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
      remote: normalizeRemoteSettings(parsed.remote),
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
  settingsWriteSequence += 1;
  window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(cachedSettings));
  if (window.__TAURI_INTERNALS__) {
    persistLatestAppSettings();
  }
}

function persistLatestAppSettings() {
  if (!window.__TAURI_INTERNALS__ || settingsWriteInFlight) return;
  const settings = cachedSettings;
  if (!settings || persistedSettingsSequence === settingsWriteSequence) return;
  const sequence = settingsWriteSequence;
  settingsWriteInFlight = true;
  void invoke<AppSettings>("app_settings_set", { settings })
    .then((persisted) => {
      if (sequence !== settingsWriteSequence) return;
      persistedSettingsSequence = sequence;
      cachedSettings = normalizeAppSettings(persisted);
      window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(cachedSettings));
    })
    .catch((error) => {
      console.error("failed to persist app settings", error);
      rejectSettingsFlushWaiters(error);
    })
    .finally(() => {
      settingsWriteInFlight = false;
      if (persistedSettingsSequence === settingsWriteSequence) {
        resolveSettingsFlushWaiters(readAppSettings());
      } else {
        persistLatestAppSettings();
      }
    });
}

export async function flushAppSettings() {
  if (!window.__TAURI_INTERNALS__) return readAppSettings();
  if (persistedSettingsSequence === settingsWriteSequence && !settingsWriteInFlight) {
    return readAppSettings();
  }
  persistLatestAppSettings();
  return new Promise<AppSettings>((resolve, reject) => {
    settingsFlushWaiters.push({ resolve, reject });
  });
}

function resolveSettingsFlushWaiters(settings: AppSettings) {
  const waiters = settingsFlushWaiters;
  settingsFlushWaiters = [];
  for (const waiter of waiters) {
    waiter.resolve(settings);
  }
}

function rejectSettingsFlushWaiters(error: unknown) {
  const waiters = settingsFlushWaiters;
  settingsFlushWaiters = [];
  for (const waiter of waiters) {
    waiter.reject(error);
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
      if (settingsWriteSequence !== persistedSettingsSequence) {
        return readAppSettings();
      }
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
    if (settingsWriteSequence !== persistedSettingsSequence) return;
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
  update.channel = update.channel === "beta" ? "beta" : "stable";
  update.endpoint = normalizeUpdateEndpoint(update.endpoint, update.enabled, update.channel);
  const themeColor = normalizeThemeColorSetting(settings.themeColor);
  return {
    ...defaultSettings,
    ...settings,
    themeColor,
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
    remote: normalizeRemoteSettings(settings.remote),
    pet: {
      ...defaultSettings.pet,
      ...(settings.pet ?? {}),
    },
    ai: normalizeAISettings(settings.ai, settings.pet),
    statisticsMode: normalizeStatisticsMode(settings.statisticsMode),
  };
}

const themeColorSettingLabels = [
  "Blue",
  "Sky",
  "Cyan",
  "Teal",
  "Emerald",
  "Green",
  "Lime",
  "Amber",
  "Orange",
  "Red",
  "Rose",
  "Pink",
  "Fuchsia",
  "Purple",
  "Violet",
  "Indigo",
] as const;

const legacyThemeColorAliases: Record<string, string> = {
  burnt: "Orange",
  crimson: "Red",
  gold: "Amber",
  iris: "Violet",
  lavender: "Violet",
  moss: "Emerald",
  navy: "Blue",
  plum: "Rose",
  sage: "Green",
};

function normalizeAppearanceName(value?: string) {
  return (value ?? "").trim().toLowerCase().replace(/[_-]+/g, " ").replace(/\s+/g, " ");
}

function canonicalAppearanceLabel(value: string | undefined, labels: readonly string[]) {
  const normalized = normalizeAppearanceName(value);
  return labels.find((label) => normalizeAppearanceName(label) === normalized);
}

function normalizeThemeColorSetting(value?: string) {
  const direct = canonicalAppearanceLabel(value, themeColorSettingLabels);
  if (direct) return direct;
  const alias = legacyThemeColorAliases[normalizeAppearanceName(value)];
  return canonicalAppearanceLabel(alias, themeColorSettingLabels) ?? defaultSettings.themeColor;
}

export function normalizeStatisticsMode(value?: string): AIStatisticsMode {
  return value === "includingCache" ? "includingCache" : "normalized";
}

function normalizeRemoteSettings(settings?: Partial<RemoteSettings>): RemoteSettings {
  const raw = settings as Partial<RemoteSettings> & { hostId?: string };
  return {
    isEnabled: Boolean(raw?.isEnabled),
    serverURL: (raw?.serverURL ?? defaultSettings.remote.serverURL).trim(),
    hostID: (raw?.hostID ?? raw?.hostId ?? "").trim(),
    hostToken: raw?.hostToken ?? "",
    hostPrivateKey: raw?.hostPrivateKey ?? "",
    hostPublicKey: raw?.hostPublicKey ?? "",
    cachedDevices: Array.isArray(raw?.cachedDevices) ? raw.cachedDevices.map(normalizeRemoteDeviceSettings) : [],
  };
}

function normalizeRemoteDeviceSettings(device: Partial<RemoteDeviceSettings>): RemoteDeviceSettings {
  return {
    id: device.id ?? "",
    hostId: device.hostId ?? "",
    name: device.name ?? "",
    publicKey: device.publicKey ?? "",
    createdAt: device.createdAt ?? "",
    lastSeen: device.lastSeen ?? "",
    revokedAt: device.revokedAt ?? null,
    online: device.online ?? null,
  };
}

function normalizeUpdateEndpoint(endpoint: string, enabled: boolean, channel: string) {
  const trimmed = endpoint.trim();
  if (!enabled) return trimmed;
  const channelEndpoint = channel === "beta" ? UPDATE_CHANNEL_ENDPOINTS.beta : UPDATE_CHANNEL_ENDPOINTS.stable;
  if (!trimmed || LEGACY_UPDATE_ENDPOINTS.has(trimmed) || isManagedUpdateEndpoint(trimmed)) return channelEndpoint;
  return trimmed;
}

function isManagedUpdateEndpoint(endpoint: string) {
  return Object.values(UPDATE_CHANNEL_ENDPOINTS).includes(endpoint);
}

function normalizeAISettings(settings?: Partial<AISettings>, legacyPet?: Partial<PetSettings>): AISettings {
  const rawPet: Partial<AIPetSettings> = settings?.pet ?? {};
  const legacySpeechMode =
    rawPet.speechMode === undefined && typeof legacyPet?.speechMode === "string" ? legacyPet.speechMode : undefined;
  const legacySpeechFrequency =
    rawPet.speechFrequency === undefined && typeof legacyPet?.speechFrequency === "string"
      ? legacyPet.speechFrequency
      : undefined;
  return {
    ...defaultSettings.ai,
    ...(settings ?? {}),
    gitCommitMessageProviderId: normalizeProviderSelector(settings?.gitCommitMessageProviderId),
    gitCommitMessageTone: normalizeGitCommitMessageTone(settings?.gitCommitMessageTone),
    gitCommitMessageLanguage: normalizeGitCommitMessageLanguage(settings?.gitCommitMessageLanguage),
    gitCommitMessageStyleRules: normalizeBoundedText(settings?.gitCommitMessageStyleRules, 4000),
    runtimeTools: normalizeRuntimeTools(settings?.runtimeTools),
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

function normalizeGitCommitMessageTone(value: unknown) {
  return typeof value === "string" && ["conventional", "concise", "sentence", "changelog"].includes(value.trim())
    ? value.trim()
    : "conventional";
}

function normalizeProviderSelector(value: unknown) {
  const normalized = typeof value === "string" ? value.trim().slice(0, 120) : "";
  return normalized || "automatic";
}

function normalizeGitCommitMessageLanguage(value: unknown) {
  return typeof value === "string" &&
    [
      "application",
      "simplifiedChinese",
      "traditionalChinese",
      "english",
      "japanese",
      "korean",
      "french",
      "german",
      "spanish",
      "portugueseBrazil",
      "russian",
    ].includes(value.trim())
    ? value.trim()
    : "application";
}

function normalizeBoundedText(value: unknown, maxLength: number) {
  return typeof value === "string" ? value.trim().slice(0, maxLength) : "";
}

function normalizeRuntimeTools(settings?: Partial<AIRuntimeToolSettings>): AIRuntimeToolSettings {
  return {
    ...defaultSettings.ai.runtimeTools,
    ...(settings ?? {}),
    codex: normalizePermissionMode(settings?.codex),
    claudeCode: normalizePermissionMode(settings?.claudeCode),
    gemini: normalizePermissionMode(settings?.gemini),
    opencode: normalizePermissionMode(settings?.opencode),
    kiro: normalizePermissionMode(settings?.kiro),
    codexEffort: normalizeCodexEffort(settings?.codexEffort),
  };
}

function normalizePermissionMode(value: unknown): AIToolPermissionMode {
  return value === "fullAccess" ? "fullAccess" : "default";
}

function normalizeCodexEffort(value: unknown): AICodexReasoningEffort {
  return value === "none" || value === "minimal" || value === "low" || value === "high" || value === "xhigh"
    ? value
    : "medium";
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
