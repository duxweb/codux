import {
  Bot,
  KeyRound,
  Palette,
  QrCode,
  Radio,
  RefreshCw,
  Server,
  Settings,
  ShieldCheck,
  Smile,
  Sparkles,
  Trash,
  Wrench,
  X,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import QRCode from "qrcode";
import { checkForUpdates, type UpdateInstallResult, type UpdateStatus } from "../appActions";
import { Button } from "../components/Button";
import { AppIconMark, appIconStyles } from "../components/AppIconMark";
import { PressableButton } from "../components/PressableButton";
import { WindowsWindowControls } from "../components/WindowsWindowControls";
import { Field, SettingsCard, FormRow, Select, SettingsForm, Textarea, TextInput, Toggle } from "../components/Form";
import { revealCurrentAppWindow } from "../windowing";
import { isWindowsPlatform } from "../platform";
import { startWindowDrag } from "../windowDrag";
import type { AppIcon } from "../icons";
import {
  readAppSettings,
  flushAppSettings,
  subscribeAppSettings,
  syncAppSettingsFromRust,
  updateAppSettings,
  type AppSettings,
  type AICodexReasoningEffort,
  type AIProviderSettings,
  type AIToolPermissionMode,
  type NotificationChannelSettings,
  type RemoteDeviceSettings,
} from "../settings";
import { appShortcutDefinitions, serializeShortcutSequence, shortcutDisplayValue } from "../shortcuts";
import { restartNotice, systemMessage } from "../systemDialog";
import {
  backgroundColorOptions,
  resolveBackgroundColorOption,
  resolveThemeColorOption,
  systemThemeOptions,
  terminalThemeOptions,
  terminalThemePreview,
  themeColorOptions,
} from "../theme";
import { formatI18n, tm } from "../i18n";
import type { RemotePendingPairing, RemoteStatus } from "../types";

type SectionId =
  | "general"
  | "appearance"
  | "pet"
  | "ai"
  | "notifications"
  | "remote"
  | "shortcuts"
  | "experiments"
  | "developer";

type Section = {
  id: SectionId;
  labelKey?: string;
  label: string;
  description: string;
  icon: AppIcon;
};

const sections: Section[] = [
  {
    id: "general",
    labelKey: "settings.tab.general",
    label: "General",
    description: "settings.tab.general.description",
    icon: Settings,
  },
  {
    id: "appearance",
    labelKey: "settings.tab.appearance",
    label: "Appearance",
    description: "settings.tab.appearance.description",
    icon: Palette,
  },
  { id: "pet", labelKey: "settings.tab.pet", label: "Pet", description: "settings.tab.pet.description", icon: Smile },
  { id: "ai", labelKey: "settings.tab.ai", label: "AI", description: "settings.tab.ai.description", icon: Bot },
  {
    id: "notifications",
    labelKey: "settings.tab.notifications",
    label: "Notifications",
    description: "settings.tab.notifications.description",
    icon: Radio,
  },
  {
    id: "remote",
    labelKey: "settings.tab.remote",
    label: "Remote",
    description: "settings.tab.remote.description",
    icon: Server,
  },
  {
    id: "shortcuts",
    labelKey: "settings.tab.shortcuts",
    label: "Shortcuts",
    description: "settings.tab.shortcuts.description",
    icon: KeyRound,
  },
  {
    id: "experiments",
    labelKey: "settings.tab.experiments",
    label: "Experiments",
    description: "settings.tab.experiments.description",
    icon: Sparkles,
  },
  {
    id: "developer",
    labelKey: "settings.tab.developer",
    label: "Developer",
    description: "settings.tab.developer.description",
    icon: Wrench,
  },
];

const languageOptions = [
  { value: "system", label: tm("settings.language.follow_system", "Follow System") },
  { value: "simplifiedChinese", label: "简体中文" },
  { value: "traditionalChinese", label: "繁體中文" },
  { value: "english", label: "English" },
  { value: "japanese", label: "日本語" },
  { value: "korean", label: "한국어" },
  { value: "french", label: "Français" },
  { value: "german", label: "Deutsch" },
  { value: "spanish", label: "Español" },
  { value: "portugueseBrazil", label: "Português (Brasil)" },
  { value: "russian", label: "Русский" },
];

const shellOptions = shellOptionsForPlatform();
const gitRefreshOptions = intervalOptions([30, 60, 120, 300, 600]);
const aiRefreshOptions = intervalOptions([60, 120, 180, 300, 600]);
const aiBackgroundRefreshOptions = intervalOptions([300, 600, 900, 1800]);
const monitorRefreshOptions = intervalOptions([1, 2, 3, 5, 10]);
const petSpeechModeOptions = ["mixed", "off", "encourage", "roast", "flirty", "chuunibyou"].map((value) => ({
  value,
  label: tm(`pet.speech.mode.${value}`, value),
}));
const petSpeechFrequencyOptions = ["quiet", "normal", "lively", "chatterbox"].map((value) => ({
  value,
  label: petSpeechFrequencyOptionLabel(value),
}));

const sleepPreventionOptions = [
  { value: "off", labelKey: "settings.sleep_prevention.mode.off", label: "Off" },
  { value: "always", labelKey: "settings.sleep_prevention.mode.always", label: "Always" },
  { value: "powerAdapterOnly", labelKey: "settings.sleep_prevention.mode.power_adapter_only", label: "On Power Only" },
];

const statisticsModeOptions = [
  { value: "normalized", label: tm("settings.ai_statistics_mode.normalized", "Exclude Cache") },
  { value: "includingCache", label: tm("settings.ai_statistics_mode.including_cache", "Include Cache") },
];

const updateChannelOptions = [
  { value: "stable", label: "Stable" },
  { value: "beta", label: "Beta" },
  { value: "nightly", label: "Nightly" },
];

const runtimeTools = [
  { id: "codex", permissionKey: "codex", modelKey: "codexModel", label: "Codex", model: "gpt-5.5" },
  {
    id: "claudeCode",
    permissionKey: "claudeCode",
    modelKey: "claudeCodeModel",
    label: "Claude Code",
    model: "claude-sonnet-4.5",
  },
  { id: "gemini", permissionKey: "gemini", modelKey: "geminiModel", label: "Gemini", model: "gemini-2.5-pro" },
  { id: "opencode", permissionKey: "opencode", modelKey: "opencodeModel", label: "OpenCode", model: "gpt-5.5" },
] as const;

const toolPermissionOptions = [
  { value: "default", label: tm("settings.tools.permission.default", "Default") },
  { value: "fullAccess", label: tm("settings.tools.permission.full_access", "Full Access") },
];

const codexEffortOptions = [
  { value: "none", label: tm("agent.effort.none", "None") },
  { value: "minimal", label: tm("agent.effort.minimal", "Minimal") },
  { value: "low", label: tm("agent.effort.low", "Low") },
  { value: "medium", label: tm("agent.effort.medium", "Medium") },
  { value: "high", label: tm("agent.effort.high", "High") },
  { value: "xhigh", label: tm("agent.effort.xhigh", "XHigh") },
];

const aiProviderKindOptions = [
  { value: "openAICompatible", label: "OpenAI-Compatible API" },
  { value: "anthropic", label: "Claude API" },
];

const memoryUserWorkingOptions = numberOptions(0, 24);
const memoryProjectWorkingOptions = numberOptions(0, 32);

const aiProviderDefaults = {
  openAICompatible: {
    displayName: "OpenAI API",
    model: "gpt-4.1-mini",
    baseUrl: "https://api.openai.com/v1",
  },
  anthropic: {
    displayName: "Claude API",
    model: "claude-3-5-haiku-latest",
    baseUrl: "https://api.anthropic.com/v1",
  },
  localLlama: {
    displayName: "Llama Model",
    model: "qwen2.5-coder-1.5b-instruct-q4_k_m",
    baseUrl: "",
  },
} satisfies Record<AIProviderSettings["kind"], { displayName: string; model: string; baseUrl: string }>;

const darkTerminalThemes = terminalThemeOptions.filter((preset) => {
  if (preset.value === "Auto") return false;
  return terminalThemePreview(preset.value).appTheme !== "light";
});
const lightTerminalThemes = terminalThemeOptions.filter(
  (preset) => terminalThemePreview(preset.value).appTheme === "light",
);

const notificationChannels = [
  ["bark", "Bark", "Server URL", "Device Key", "Send push alerts through a Bark server with your device key."],
  ["ntfy", "ntfy", "Topic URL", "Bearer Token", "Publish messages to an ntfy topic."],
  ["wxpusher", "WxPusher", "SPT Token", "Token", "Send notifications to a WxPusher SPT target."],
  ["feishu", "Feishu", "Webhook URL", "Hook Token", "Post messages with a Feishu bot webhook."],
  ["dingtalk", "DingTalk", "Webhook URL", "Access Token", "Post messages with a DingTalk robot webhook."],
  ["wecom", "WeCom", "Webhook URL", "Webhook Key", "Post messages to a WeCom group bot."],
  ["telegram", "Telegram", "Chat ID", "Bot Token", "Send messages with a Telegram bot token and target chat ID."],
  ["discord", "Discord", "Webhook URL", "Optional Auth Token", "Deliver notifications to a Discord webhook."],
  ["slack", "Slack", "Webhook URL", "Optional Auth Token", "Deliver notifications to a Slack incoming webhook."],
  ["webhook", "Webhook", "Request URL", "Bearer Token", "Send JSON POST requests to your own endpoint."],
] as const;

export function SettingsWindow() {
  const [active, setActive] = useState<SectionId>("general");
  const activeSection = sections.find((item) => item.id === active) ?? sections[0];
  const titleRightInset = isWindowsPlatform() ? "pr-[58px]" : "pr-6";

  useEffect(() => {
    void revealCurrentAppWindow();
  }, []);

  return (
    <div className="app-shell h-screen grid grid-cols-[200px_minmax(0,1fr)] text-ink">
      {isWindowsPlatform() && <WindowsWindowControls closeOnly className="h-12" />}
      <aside className="min-h-0 border-r border-line bg-surface-chrome/45 flex flex-col">
        <div className="h-14 flex-shrink-0 drag-region" data-tauri-drag-region onPointerDown={startWindowDrag} />
        <nav className="min-h-0 flex-1 overflow-y-auto px-3 pb-3 no-drag">
          <div className="grid gap-1.5">
            {sections.map((item) => (
              <PressableButton
                key={item.id}
                onPressUp={() => setActive(item.id)}
                className={`h-8 px-2.5 rounded-md grid grid-cols-[18px_minmax(0,1fr)] items-center gap-2 text-sm text-left transition-colors ${
                  active === item.id ? "bg-brand-blue/16 text-ink" : "text-ink-soft hover:bg-fill/[0.06] hover:text-ink"
                }`}
              >
                <item.icon size={14} strokeWidth={2} />
                <span>{tm(item.labelKey ?? "", item.label)}</span>
              </PressableButton>
            ))}
          </div>
        </nav>
      </aside>

      <section className="relative min-w-0 min-h-0 overflow-hidden">
        <header
          className={`absolute left-0 right-0 top-0 z-20 h-[92px] flex flex-col justify-end pl-6 ${titleRightInset} pb-4 drag-region`}
          data-tauri-drag-region
          onPointerDown={startWindowDrag}
          style={{
            background: "linear-gradient(to top, transparent 0%, var(--color-surface-chrome) 50%)",
          }}
        >
          <div className="text-lg font-semibold tracking-tight drag-region" data-tauri-drag-region>
            {tm(activeSection.labelKey ?? "", activeSection.label)}
          </div>
          <div className="mt-1.5 text-sm text-ink-mute drag-region" data-tauri-drag-region>
            {tm(activeSection.description, activeSection.description)}
          </div>
        </header>

        <main className="absolute inset-0 overflow-y-auto px-5 pt-[108px] pb-6 no-drag">
          <SettingsPane active={active} />
        </main>
      </section>
    </div>
  );
}

function SettingsPane({ active }: { active: SectionId }) {
  switch (active) {
    case "general":
      return <GeneralSection />;
    case "appearance":
      return <AppearanceSection />;
    case "pet":
      return <PetSection />;
    case "ai":
      return <AISection />;
    case "notifications":
      return <NotificationSection />;
    case "remote":
      return <RemoteSection />;
    case "shortcuts":
      return <ShortcutSection />;
    case "experiments":
      return <ExperimentSection />;
    case "developer":
      return <DeveloperSection />;
  }
}

function useSyncedSettings() {
  const [settings, setSettings] = useState<AppSettings>(readAppSettings);
  useEffect(() => {
    void syncAppSettingsFromRust().then(setSettings);
    return subscribeAppSettings(setSettings);
  }, []);
  return [settings, setSettings] as const;
}

function GeneralSection() {
  const [settings, setSettings] = useSyncedSettings();
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [isCheckingUpdates, setCheckingUpdates] = useState(false);
  const [isInstallingUpdate, setInstallingUpdate] = useState(false);
  const setSetting = <K extends keyof typeof settings>(key: K, value: (typeof settings)[K]) => {
    const next = updateAppSettings({ [key]: value });
    setSettings(next);
    if (key === "language") {
      void restartNotice(
        tm("settings.language.restart_message", "Restart Codux to apply the selected language."),
        tm("settings.language.restart_title", "Restart Required"),
      );
    }
  };
  const refreshUpdateStatus = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__) return;
    setCheckingUpdates(true);
    try {
      setUpdateStatus(await invoke<UpdateStatus>("app_update_status"));
    } catch (error) {
      setUpdateStatus({
        configured: false,
        checking: false,
        available: false,
        automaticInstallSupported: false,
        signedUpdaterConfigured: false,
        manifestEndpointConfigured: false,
        currentVersion: "",
        latestVersion: null,
        downloadUrl: null,
        channel: settings.update.channel,
        installationMode: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setCheckingUpdates(false);
    }
  }, [settings.update.channel]);
  const installUpdate = async () => {
    if (!window.__TAURI_INTERNALS__) return;
    setInstallingUpdate(true);
    try {
      const result = await invoke<UpdateInstallResult>("app_update_install");
      await systemMessage(result.message, {
        title: tm("update.installed.title", "Update Installed"),
        kind: "info",
        buttons: { ok: "OK" },
      });
      await refreshUpdateStatus();
    } catch (error) {
      await systemMessage(error instanceof Error ? error.message : String(error), {
        title: tm("update.install_failed.title", "Unable to Install Update"),
        kind: "warning",
        buttons: { ok: "OK" },
      });
    } finally {
      setInstallingUpdate(false);
    }
  };

  useEffect(() => {
    void refreshUpdateStatus();
  }, [refreshUpdateStatus, settings.update.enabled]);

  return (
    <SettingsForm className="max-w-[640px]">
      <SettingsCard>
        <Field label={tm("settings.language", "Language")}>
          <Select
            value={settings.language}
            onChange={(value) => setSetting("language", value)}
            options={languageOptions}
          />
        </Field>
        <Field label={tm("settings.default_shell", "Default Shell")}>
          <Select value={settings.shell} onChange={(value) => setSetting("shell", value)} options={shellOptions} />
        </Field>
        <FormRow label={tm("settings.dock_badge", "Dock Badge")}>
          <Toggle checked={settings.showsDockBadge} onChange={(value) => setSetting("showsDockBadge", value)} />
        </FormRow>
        <Field
          label={tm("settings.sleep_prevention", "Prevent System Sleep")}
          description={tm(
            "settings.sleep_prevention.help",
            "Allows the display to turn off, but prevents this device from idle sleeping while enabled.",
          )}
        >
          <Select
            value={settings.sleepMode}
            onChange={(value) => setSetting("sleepMode", value)}
            options={sleepPreventionOptions.map((option) => ({
              value: option.value,
              label: tm(option.labelKey, option.label),
            }))}
          />
        </Field>
      </SettingsCard>

      <SettingsCard>
        <Field label={tm("settings.git_auto_refresh", "Git Auto Refresh")}>
          <Select
            value={settings.gitRefresh}
            onChange={(value) => setSetting("gitRefresh", value)}
            options={gitRefreshOptions}
          />
        </Field>
        <Field label={tm("settings.ai_auto_refresh", "AI Auto Refresh")}>
          <Select
            value={settings.aiRefresh}
            onChange={(value) => setSetting("aiRefresh", value)}
            options={aiRefreshOptions}
          />
        </Field>
        <Field label={tm("settings.ai_background_refresh", "AI Background Refresh")}>
          <Select
            value={settings.aiBackgroundRefresh}
            onChange={(value) => setSetting("aiBackgroundRefresh", value)}
            options={aiBackgroundRefreshOptions}
          />
        </Field>
        <Field label={tm("settings.ai_statistics_mode", "AI Statistics Mode")}>
          <Select
            value={settings.statisticsMode}
            onChange={(value) => setSetting("statisticsMode", value)}
            options={statisticsModeOptions}
          />
        </Field>
      </SettingsCard>

      <SettingsCard
        title={tm("settings.update.section", "Updates")}
        description={tm(
          "settings.update.description",
          "Updates use signed Tauri artifacts from the tauri-stable GitHub Release.",
        )}
      >
        <FormRow label={tm("settings.update.enabled", "Enable Update Checks")}>
          <Toggle
            checked={settings.update.enabled}
            onChange={(enabled) => setSetting("update", { ...settings.update, enabled })}
          />
        </FormRow>
        <Field label={tm("settings.update.channel", "Update Channel")}>
          <Select
            value={settings.update.channel}
            onChange={(channel) => setSetting("update", { ...settings.update, channel })}
            options={updateChannelOptions}
          />
        </Field>
        <FormRow
          label={tm("settings.update.status", "Update Status")}
          description={updateStatusDescription(updateStatus, isCheckingUpdates)}
        >
          <div className="flex items-center gap-2">
            <span className="rounded-md border border-line bg-fill/[0.04] px-2 py-1 text-xs font-medium text-ink-faint">
              {updateModeLabel(updateStatus)}
            </span>
            <Button
              size="sm"
              variant="secondary"
              disabled={isCheckingUpdates || !settings.update.enabled}
              onPress={() => void checkForUpdates().then(refreshUpdateStatus)}
            >
              {isCheckingUpdates ? tm("update.checking", "Checking...") : tm("about.updates", "Check for Updates")}
            </Button>
            {updateStatus?.available && updateStatus.automaticInstallSupported && (
              <Button size="sm" variant="primary" disabled={isInstallingUpdate} onPress={() => void installUpdate()}>
                {isInstallingUpdate
                  ? tm("update.installing", "Installing...")
                  : tm("update.available.install", "Install")}
              </Button>
            )}
          </div>
        </FormRow>
      </SettingsCard>
    </SettingsForm>
  );
}

function updateModeLabel(status: UpdateStatus | null) {
  if (!status) return tm("update.checking", "Checking");
  if (status.installationMode === "disabled") {
    return tm("settings.update.mode.disabled", "Off");
  }
  if (status.automaticInstallSupported) {
    return tm("settings.update.mode.automatic", "Automatic");
  }
  if (status.manifestEndpointConfigured) {
    return tm("settings.update.mode.manual_manifest", "Manual");
  }
  if (status.signedUpdaterConfigured) {
    return tm("settings.update.mode.signed_config", "Signed Config");
  }
  if (!status.configured) return tm("settings.update.mode.manual_manifest", "Manual");
  return status.installationMode;
}

function updateStatusDescription(status: UpdateStatus | null, checking: boolean) {
  if (checking || !status) {
    return tm("settings.update.status.checking_github", "Checking GitHub releases...");
  }
  if (status.installationMode === "disabled") {
    return tm("settings.update.status.disabled", "Update checks are turned off.");
  }
  if (status.available) {
    return formatI18n(
      tm("settings.update.status.available_format", "Version %@ is available. Current version: %@."),
      status.latestVersion ?? status.currentVersion,
      status.currentVersion,
    );
  }
  if (status.latestVersion) {
    return formatI18n(
      tm("settings.update.status.latest_format", "Current version %@ is up to date."),
      status.currentVersion,
    );
  }
  return tm("settings.update.status.error", "Unable to check updates. Please try again later.");
}

function shellOptionsForPlatform() {
  const platform = typeof navigator === "undefined" ? "" : navigator.platform.toLowerCase();
  const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent.toLowerCase();
  const isWindows = platform.includes("win") || userAgent.includes("windows");
  const isMac = platform.includes("mac");
  const shells = isWindows
    ? [
        { value: "pwsh.exe", label: "PowerShell 7" },
        { value: "powershell.exe", label: "Windows PowerShell" },
      ]
    : isMac
      ? ["zsh", "bash", "sh", "fish"].map((value) => ({ value, label: value }))
      : ["bash", "sh", "zsh", "fish"].map((value) => ({ value, label: value }));
  return [{ value: "system", label: tm("settings.default_shell.system", "Follow System") }, ...shells];
}

function AppearanceSection() {
  const [settings, setSettings] = useSyncedSettings();
  const selectedBackgroundColor = resolveBackgroundColorOption(settings.background)?.label ?? settings.background;
  const selectedThemeColor = resolveThemeColorOption(settings.themeColor)?.label ?? settings.themeColor;
  const setSetting = <K extends keyof typeof settings>(key: K, value: (typeof settings)[K]) => {
    const next = updateAppSettings({ [key]: value });
    setSettings(next);
    if (key === "theme") {
      void restartNotice(
        tm("settings.theme.restart_message", "Restart Codux to apply the selected theme to the app and all terminals."),
        tm("settings.theme.restart_title", "Restart Required"),
      );
    }
  };

  return (
    <SettingsForm className="max-w-[720px]">
      <SettingsCard
        title={tm("settings.terminal_theme", "Terminal Theme")}
        description={tm(
          "settings.terminal_theme.restart_pending",
          "Terminal and editor theme changes will apply after restart.",
        )}
      >
        <div className="grid gap-4 py-1">
          <ThemePreviewGrid
            presets={systemThemeOptions}
            selected={settings.theme}
            onSelect={(theme) => setSetting("theme", theme)}
          />
          <ThemePreviewGrid
            title={tm("settings.theme.group.dark", "Dark")}
            presets={darkTerminalThemes}
            selected={settings.theme}
            onSelect={(theme) => setSetting("theme", theme)}
          />
          <ThemePreviewGrid
            title={tm("settings.theme.group.light", "Light")}
            presets={lightTerminalThemes}
            selected={settings.theme}
            onSelect={(theme) => setSetting("theme", theme)}
          />
        </div>
      </SettingsCard>

      <SettingsCard
        title={tm("settings.background_color", "Background Color")}
        description={tm(
          "settings.background_color.help",
          "Auto follows the terminal background. Manual choices tint only the main window glass and panels.",
        )}
      >
        <div className="grid grid-cols-[repeat(auto-fill,minmax(48px,1fr))] gap-x-2 gap-y-3 py-1">
          {backgroundColorOptions.map((item) => (
            <ColorSwatchButton
              key={item.label}
              label={item.label}
              color={item.preview}
              selected={selectedBackgroundColor === item.label}
              onPress={() => setSetting("background", item.label)}
            />
          ))}
        </div>
      </SettingsCard>

      <SettingsCard
        title={tm("settings.theme_color", "Theme Color")}
        description={tm(
          "settings.theme_color.help",
          "Applies to buttons, selected states, tabs, focus rings, links, and other highlights.",
        )}
      >
        <div className="grid grid-cols-[repeat(auto-fill,minmax(48px,1fr))] gap-x-2 gap-y-3 py-1">
          {themeColorOptions.map((item) => (
            <ColorSwatchButton
              key={item.label}
              label={item.label}
              color={item.color}
              selected={selectedThemeColor === item.label}
              onPress={() => setSetting("themeColor", item.label)}
            />
          ))}
        </div>
      </SettingsCard>

      <SettingsCard title={tm("settings.terminal_text", "Terminal Text")}>
        <Field label={tm("settings.terminal_font_size", "Terminal Font Size")}>
          <TextInput
            type="number"
            min={10}
            max={28}
            value={settings.terminalFontSize}
            onChange={(event) => setSetting("terminalFontSize", event.currentTarget.value)}
          />
        </Field>
      </SettingsCard>

      <SettingsCard title={tm("settings.app_icon", "App Icon")}>
        <div className="flex flex-wrap gap-4 py-1">
          {appIconStyles.map((style) => (
            <AppIconPreviewButton
              key={style.value}
              label={tm(style.labelKey, style.label)}
              styleName={style.value}
              selected={settings.iconStyle === style.value}
              onPress={() => setSetting("iconStyle", style.value)}
            />
          ))}
        </div>
      </SettingsCard>
    </SettingsForm>
  );
}

function PetSection() {
  const [settings, setSettings] = useSyncedSettings();
  const providerOptions = aiProviderOptions(settings.ai.providers, "petSpeech");
  const speechDisabled = settings.ai.pet.speechMode === "off";

  const setPetSetting = (patch: Partial<typeof settings.pet>) => {
    const nextPet = { ...settings.pet, ...patch };
    const next = updateAppSettings({ pet: nextPet });
    setSettings(next);
  };
  const setAIPet = (patch: Partial<typeof settings.ai.pet>) => {
    const next = updateAppSettings({
      ai: {
        ...settings.ai,
        pet: {
          ...settings.ai.pet,
          ...patch,
        },
      },
    });
    setSettings(next);
  };

  return (
    <SettingsForm className="max-w-[700px]">
      <SettingsCard title={tm("settings.pet.section.general", "General")}>
        <FormRow label={tm("settings.pet.enabled", "Enable Pet")}>
          <Toggle checked={settings.pet.enabled} onChange={(enabled) => setPetSetting({ enabled })} />
        </FormRow>
        <FormRow label={tm("settings.pet.desktop_widget", "Desktop Pet")}>
          <Toggle
            checked={settings.pet.desktopWidget}
            disabled={!settings.pet.enabled}
            onChange={(desktopWidget) => setPetSetting({ desktopWidget })}
          />
        </FormRow>
        <FormRow label={tm("settings.pet.static_mode", "Static Pet Sprite")}>
          <Toggle checked={settings.pet.staticMode} onChange={(staticMode) => setPetSetting({ staticMode })} />
        </FormRow>
      </SettingsCard>

      <SettingsCard title={tm("settings.pet.speech.section", "Pet Speech")}>
        <Field label={tm("settings.pet.speech.mode", "Mode")}>
          <Select
            value={settings.ai.pet.speechMode}
            onChange={(speechMode) => setAIPet({ speechMode })}
            options={petSpeechModeOptions}
          />
        </Field>
        <Field label={tm("settings.pet.speech.frequency", "Frequency")}>
          <Select
            value={settings.ai.pet.speechFrequency}
            onChange={(speechFrequency) => setAIPet({ speechFrequency })}
            options={petSpeechFrequencyOptions}
            isDisabled={speechDisabled}
          />
        </Field>
        <FormRow label={tm("settings.pet.speech.quiet_during_work", "Speak Less During Work Hours")}>
          <Toggle
            checked={settings.ai.pet.speechQuietDuringWork}
            disabled={speechDisabled}
            onChange={(speechQuietDuringWork) => setAIPet({ speechQuietDuringWork })}
          />
        </FormRow>
        <FormRow label={tm("settings.pet.speech.louder_at_night", "Speak More At Night")}>
          <Toggle
            checked={settings.ai.pet.speechLouderAtNight}
            disabled={speechDisabled}
            onChange={(speechLouderAtNight) => setAIPet({ speechLouderAtNight })}
          />
        </FormRow>
        <FormRow label={tm("settings.pet.speech.mute_on_fullscreen", "Mute In Full Screen")}>
          <Toggle
            checked={settings.ai.pet.speechMuteOnFullscreen}
            disabled={speechDisabled}
            onChange={(speechMuteOnFullscreen) => setAIPet({ speechMuteOnFullscreen })}
          />
        </FormRow>
        <FormRow label={tm("settings.pet.speech.quiet_hours", "Quiet Hours 22:00-08:00")}>
          <Toggle
            checked={settings.ai.pet.speechQuietHoursStart !== null && settings.ai.pet.speechQuietHoursEnd !== null}
            disabled={speechDisabled}
            onChange={(enabled) =>
              setAIPet({
                speechQuietHoursStart: enabled ? 22 : null,
                speechQuietHoursEnd: enabled ? 8 : null,
              })
            }
          />
        </FormRow>
        <div className="flex justify-end gap-2">
          <Button
            size="sm"
            variant="secondary"
            disabled={speechDisabled}
            onPress={() => setAIPet({ speechTemporaryMuteUntil: Math.floor(Date.now() / 1000) + 1800 })}
          >
            {tm("settings.pet.speech.mute_30_minutes", "Mute 30 Minutes")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={speechDisabled || settings.ai.pet.speechTemporaryMuteUntil === null}
            onPress={() => setAIPet({ speechTemporaryMuteUntil: null })}
          >
            {tm("settings.pet.speech.unmute", "Cancel Temporary Mute")}
          </Button>
        </div>
      </SettingsCard>

      <SettingsCard
        title={tm("settings.pet.llm.section", "Pet LLM")}
        description={tm(
          "settings.pet.llm.help",
          "Only rhythm and milestone messages try LLM polishing. Template lines are used if it fails, times out, or no LLM channel is available.",
        )}
      >
        <FormRow label={tm("settings.pet.llm.enabled", "Enable LLM Line Polishing")}>
          <Toggle
            checked={settings.ai.pet.speechLlmEnabled}
            disabled={speechDisabled}
            onChange={(speechLlmEnabled) => setAIPet({ speechLlmEnabled })}
          />
        </FormRow>
        <Field label={tm("settings.pet.llm.channel", "LLM Channel")}>
          <Select
            value={settings.ai.pet.speechProviderId}
            onChange={(speechProviderId) => setAIPet({ speechProviderId })}
            options={providerOptions}
            isDisabled={speechDisabled || !settings.ai.pet.speechLlmEnabled}
          />
        </Field>
      </SettingsCard>

      <SettingsCard title={tm("settings.pet.section.reminders", "Reminders")}>
        <FormRow label={tm("settings.pet.reminder.hydration", "Hydration Reminder")}>
          <Toggle checked={settings.pet.reminders} onChange={(reminders) => setPetSetting({ reminders })} />
        </FormRow>
      </SettingsCard>
    </SettingsForm>
  );
}

function AISection() {
  const [settings, setSettings] = useSyncedSettings();
  const [testingProviderId, setTestingProviderId] = useState<string | null>(null);
  const ai = settings.ai;
  const providers = ai.providers;
  const providerOptions = aiProviderOptions(providers, "memory");

  const setAI = (patch: Partial<typeof ai>) => {
    const next = updateAppSettings({
      ai: {
        ...ai,
        ...patch,
      },
    });
    setSettings(next);
  };
  const setRuntimeTools = (patch: Partial<typeof ai.runtimeTools>) => {
    setAI({
      runtimeTools: {
        ...ai.runtimeTools,
        ...patch,
      },
    });
  };
  const setMemory = (patch: Partial<typeof ai.memory>) => {
    setAI({ memory: { ...ai.memory, ...patch } });
  };
  const upsertProvider = (id: string, patch: Partial<AIProviderSettings>) => {
    setAI({
      providers: providers.map((provider) => (provider.id === id ? { ...provider, ...patch } : provider)),
    });
  };
  const addProvider = (kind: AIProviderSettings["kind"] = "openAICompatible") => {
    const defaults = aiProviderDefaults[kind];
    const provider: AIProviderSettings = {
      id: `api-${kind}-${crypto.randomUUID ? crypto.randomUUID() : Date.now().toString(36)}`,
      kind,
      displayName: defaults.displayName,
      isEnabled: true,
      model: defaults.model,
      baseUrl: defaults.baseUrl,
      apiKey: "",
      useForMemoryExtraction: true,
      priority: providers.length,
    };
    setAI({ providers: [...providers, provider] });
  };
  const removeProvider = (id: string) => {
    setAI({
      providers: providers.filter((provider) => provider.id !== id),
      memory: {
        ...ai.memory,
        defaultExtractorProviderId:
          ai.memory.defaultExtractorProviderId === id ? "automatic" : ai.memory.defaultExtractorProviderId,
      },
      pet: {
        ...ai.pet,
        speechProviderId: ai.pet.speechProviderId === id ? "automatic" : ai.pet.speechProviderId,
      },
    });
  };
  const testProvider = async (provider: AIProviderSettings) => {
    setTestingProviderId(provider.id);
    try {
      const result = await invoke<{ providerName: string; text: string }>("llm_provider_test", {
        provider,
      });
      await systemMessage(
        formatI18n(
          tm("settings.ai.provider.test.succeeded_format", "Test succeeded: %@"),
          result.text || result.providerName,
        ),
        { title: tm("settings.ai.provider.test", "Test"), kind: "info" },
      );
    } catch (error) {
      await systemMessage(
        formatI18n(
          tm("settings.ai.provider.test.failed_format", "Test failed: %@"),
          error instanceof Error ? error.message : String(error),
        ),
        { title: tm("settings.ai.provider.test", "Test"), kind: "error" },
      );
    } finally {
      setTestingProviderId(null);
    }
  };

  return (
    <SettingsForm className="max-w-[680px]">
      <SettingsCard title={tm("settings.ai.section.runtime_tools", "Runtime Tools")}>
        {runtimeTools.map((tool) => (
          <div key={tool.id} className="grid gap-3">
            <div className="text-sm font-semibold text-ink">
              {formatI18n(tm("settings.ai.tool.configuration_format", "%@ Configuration"), tool.label)}
            </div>
            <Field label={tm("settings.ai.permission.full_access_toggle", "Full Access")}>
              <Select
                value={ai.runtimeTools[tool.permissionKey]}
                onChange={(value) => setRuntimeTools({ [tool.permissionKey]: value as AIToolPermissionMode })}
                options={toolPermissionOptions}
              />
            </Field>
            <Field label={tm("settings.ai.tool.default_model", "Default Model")}>
              <TextInput
                placeholder={tool.model}
                value={ai.runtimeTools[tool.modelKey]}
                onChange={(event) => setRuntimeTools({ [tool.modelKey]: event.currentTarget.value })}
              />
            </Field>
            {tool.id === "codex" && (
              <Field label={tm("agent.effort.title", "Reasoning Effort")}>
                <Select
                  value={ai.runtimeTools.codexEffort}
                  onChange={(codexEffort) => setRuntimeTools({ codexEffort: codexEffort as AICodexReasoningEffort })}
                  options={codexEffortOptions}
                />
              </Field>
            )}
          </div>
        ))}
      </SettingsCard>

      <SettingsCard
        title={tm("settings.ai.global_prompt", "Global Prompt")}
        description={tm(
          "settings.ai.global_prompt_help",
          "Injected when supported tools start. It is merged with memory context and written to each tool's launch context.",
        )}
      >
        <div className="py-1">
          <Textarea
            value={ai.globalPrompt}
            rows={4}
            onChange={(event) => setAI({ globalPrompt: event.currentTarget.value })}
          />
        </div>
      </SettingsCard>

      <SettingsCard title={tm("settings.ai.section.memory", "Memory")}>
        <FormRow label={tm("settings.ai.memory.enabled", "Enable Memory")}>
          <Toggle checked={ai.memory.enabled} onChange={(enabled) => setMemory({ enabled })} />
        </FormRow>
      </SettingsCard>

      {ai.memory.enabled && (
        <SettingsCard title={tm("settings.ai.memory.automatic_injection", "Automatic Injection")}>
          <FormRow label={tm("settings.ai.memory.automatic_injection", "Automatic Injection")}>
            <Toggle
              checked={ai.memory.automaticInjectionEnabled}
              onChange={(automaticInjectionEnabled) => setMemory({ automaticInjectionEnabled })}
            />
          </FormRow>
          <FormRow label={tm("settings.ai.memory.automatic_extraction", "Automatic Extraction")}>
            <Toggle
              checked={ai.memory.automaticExtractionEnabled}
              onChange={(automaticExtractionEnabled) => setMemory({ automaticExtractionEnabled })}
            />
          </FormRow>
          <FormRow label={tm("settings.ai.memory.cross_project_user", "Cross-Project User Memory")}>
            <Toggle
              checked={ai.memory.allowCrossProjectUserRecall}
              onChange={(allowCrossProjectUserRecall) => setMemory({ allowCrossProjectUserRecall })}
            />
          </FormRow>
        </SettingsCard>
      )}

      {ai.memory.enabled && (
        <SettingsCard title={tm("settings.ai.memory.default_extraction_provider", "Default Extraction Provider")}>
          <Field label={tm("settings.ai.memory.default_extraction_provider", "Default Extraction Provider")}>
            <Select
              value={ai.memory.defaultExtractorProviderId}
              onChange={(defaultExtractorProviderId) => setMemory({ defaultExtractorProviderId })}
              options={providerOptions}
            />
          </Field>
          <Field label={tm("settings.ai.memory.user_working_recall", "User Working Recall")}>
            <Select
              value={String(ai.memory.maxInjectedUserWorkingMemories)}
              onChange={(value) => setMemory({ maxInjectedUserWorkingMemories: Number(value) })}
              options={memoryUserWorkingOptions}
            />
          </Field>
          <Field label={tm("settings.ai.memory.project_working_recall", "Project Working Recall")}>
            <Select
              value={String(ai.memory.maxInjectedProjectWorkingMemories)}
              onChange={(value) => setMemory({ maxInjectedProjectWorkingMemories: Number(value) })}
              options={memoryProjectWorkingOptions}
            />
          </Field>
        </SettingsCard>
      )}

      <SettingsCard
        title={tm("settings.ai.section.providers", "AI Providers")}
        action={
          <Button size="sm" variant="secondary" onPress={() => addProvider("openAICompatible")}>
            {tm("settings.ai.provider.add", "Add API Channel")}
          </Button>
        }
      >
        {providers.length === 0 && (
          <div className="text-sm text-ink-faint">
            {tm("settings.ai.provider.empty", "No API channel has been added yet.")}
          </div>
        )}
        {providers.map((provider) => (
          <AIProviderCard
            key={provider.id}
            provider={provider}
            testing={testingProviderId === provider.id}
            onChange={(patch) => upsertProvider(provider.id, patch)}
            onRemove={() => removeProvider(provider.id)}
            onTest={() => void testProvider(provider)}
          />
        ))}
      </SettingsCard>
    </SettingsForm>
  );
}

function AIProviderCard({
  provider,
  testing,
  onChange,
  onRemove,
  onTest,
}: {
  provider: AIProviderSettings;
  testing: boolean;
  onChange: (patch: Partial<AIProviderSettings>) => void;
  onRemove: () => void;
  onTest: () => void;
}) {
  const changeKind = (kind: string) => {
    const nextKind = kind === "anthropic" ? "anthropic" : "openAICompatible";
    const defaults = aiProviderDefaults[nextKind];
    onChange({
      kind: nextKind,
      displayName: provider.displayName || defaults.displayName,
      model: provider.model || defaults.model,
      baseUrl: provider.baseUrl || defaults.baseUrl,
    });
  };

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between gap-3">
        <div className="text-sm font-semibold text-ink">{provider.displayName}</div>
        <div className="flex items-center gap-2">
          <Toggle checked={provider.isEnabled} onChange={(isEnabled) => onChange({ isEnabled })} />
          <Button size="sm" variant="ghost" onPress={onRemove}>
            {tm("settings.ai.provider.remove", "Remove")}
          </Button>
        </div>
      </div>
      <Field label={tm("settings.ai.provider.kind", "Kind")}>
        <Select value={provider.kind} onChange={changeKind} options={aiProviderKindOptions} />
      </Field>
      <Field label={tm("settings.ai.provider.name", "Name")}>
        <TextInput
          value={provider.displayName}
          onChange={(event) => onChange({ displayName: event.currentTarget.value })}
        />
      </Field>
      <Field label={tm("settings.ai.provider.model", "Model")}>
        <TextInput value={provider.model} onChange={(event) => onChange({ model: event.currentTarget.value })} />
      </Field>
      <Field label={tm("settings.ai.provider.base_url", "Base URL")}>
        <TextInput value={provider.baseUrl} onChange={(event) => onChange({ baseUrl: event.currentTarget.value })} />
      </Field>
      <Field label={tm("settings.ai.provider.api_key", "API Key")}>
        <TextInput
          type="password"
          value={provider.apiKey}
          onChange={(event) => onChange({ apiKey: event.currentTarget.value })}
        />
      </Field>
      <FormRow label={tm("settings.ai.provider.use_for_memory_extraction", "Use For Memory Extraction")}>
        <Toggle
          checked={provider.useForMemoryExtraction}
          onChange={(useForMemoryExtraction) => onChange({ useForMemoryExtraction })}
        />
      </FormRow>
      <div className="flex justify-end">
        <Button
          size="sm"
          variant="secondary"
          disabled={testing || !provider.apiKey.trim() || provider.kind === "localLlama"}
          onPress={onTest}
        >
          {testing ? tm("settings.ai.provider.test.running", "Testing...") : tm("settings.ai.provider.test", "Test")}
        </Button>
      </div>
    </div>
  );
}

function aiProviderOptions(providers: AIProviderSettings[], purpose: "memory" | "petSpeech") {
  return [
    { value: "automatic", label: tm("settings.ai.memory.extraction_provider.automatic", "Automatic") },
    ...providers
      .filter(
        (provider) =>
          provider.isEnabled &&
          (purpose !== "memory" || provider.useForMemoryExtraction) &&
          provider.kind !== "localLlama",
      )
      .sort((left, right) => left.priority - right.priority || left.displayName.localeCompare(right.displayName))
      .map((provider) => ({ value: provider.id, label: provider.displayName })),
  ];
}

function numberOptions(min: number, max: number) {
  return Array.from({ length: max - min + 1 }, (_, index) => {
    const value = String(min + index);
    return { value, label: value };
  });
}

function petSpeechFrequencyOptionLabel(value: string) {
  const config = petSpeechFrequencyConfig(value);
  const cooldown =
    config.cooldownSeconds >= 60
      ? formatI18n(tm("settings.pet.speech.cooldown.minutes_format", "%d min"), Math.round(config.cooldownSeconds / 60))
      : formatI18n(tm("settings.pet.speech.cooldown.seconds_format", "%d sec"), config.cooldownSeconds);
  return formatI18n(
    tm("settings.pet.speech.frequency_option_format", "%@ · %@/hour · cooldown %@"),
    tm(`pet.speech.frequency.${value}`, value),
    config.hourly,
    cooldown,
  );
}

function petSpeechFrequencyConfig(value: string) {
  switch (value) {
    case "quiet":
      return { hourly: "0-1", cooldownSeconds: 300 };
    case "lively":
      return { hourly: "3-8", cooldownSeconds: 30 };
    case "chatterbox":
      return { hourly: "8-15", cooldownSeconds: 30 };
    default:
      return { hourly: "1-3", cooldownSeconds: 60 };
  }
}

function NotificationSection() {
  const [settings, setSettings] = useSyncedSettings();
  const [testingChannelId, setTestingChannelId] = useState<string | null>(null);
  const updateChannel = (id: string, patch: Partial<NotificationChannelSettings>) => {
    const next = updateAppSettings({
      notificationChannels: {
        ...settings.notificationChannels,
        [id]: {
          ...(settings.notificationChannels[id] ?? {}),
          enabled: false,
          endpoint: "",
          token: "",
          ...patch,
        },
      },
    });
    setSettings(next);
  };
  const testChannel = async (id: string, title: string, channel: NotificationChannelSettings) => {
    if (!channel.endpoint.trim()) return;
    setTestingChannelId(id);
    try {
      await invoke("notification_dispatch_channels", {
        request: {
          channels: [
            {
              id,
              endpoint: channel.endpoint.trim(),
              token: channel.token.trim(),
            },
          ],
          title: tm("settings.ai.provider.test", "Test"),
          body: formatI18n(tm("settings.ai.provider.test.succeeded_format", "Test succeeded: %@"), title),
          group: "codux-test",
        },
      });
      await systemMessage(formatI18n(tm("settings.ai.provider.test.succeeded_format", "Test succeeded: %@"), title), {
        title: tm("settings.ai.provider.test", "Test"),
        kind: "info",
      });
    } catch (error) {
      console.error("failed to test notification channel", error);
      await systemMessage(
        formatI18n(
          tm("settings.ai.provider.test.failed_format", "Test failed: %@"),
          error instanceof Error ? error.message : String(error),
        ),
        {
          title: tm("settings.ai.provider.test", "Test"),
          kind: "error",
        },
      );
    } finally {
      setTestingChannelId(null);
    }
  };

  return (
    <SettingsForm className="max-w-[720px]">
      {notificationChannels.map(([id, title, endpointLabel, tokenLabel, description]) => {
        const label = tm(`settings.notifications.channel.${id}.title`, title);
        const endpoint = tm(`settings.notifications.channel.${id}.endpoint`, endpointLabel);
        const token = tm(`settings.notifications.channel.${id}.token`, tokenLabel);
        const detail = tm(`settings.notifications.channel.${id}.description`, description);
        const channel = settings.notificationChannels[id] ?? {
          enabled: false,
          endpoint: "",
          token: "",
        };
        return (
          <SettingsCard key={id}>
            <div className="flex items-start gap-3">
              <div className="mt-0.5 grid h-8 w-8 place-items-center rounded-md bg-brand-blue/14 text-brand-blue">
                <Radio size={15} strokeWidth={2} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-3">
                  <div className="text-sm font-semibold text-ink">{label}</div>
                  <Toggle checked={channel.enabled} onChange={(enabled) => updateChannel(id, { enabled })} />
                </div>
                <div className="mt-1 text-xs text-ink-faint">{detail}</div>
              </div>
            </div>
            {channel.enabled && (
              <div className="grid gap-3 pt-2">
                <Field label={endpoint}>
                  <TextInput
                    placeholder={endpoint}
                    value={channel.endpoint}
                    onChange={(event) => updateChannel(id, { endpoint: event.currentTarget.value })}
                  />
                </Field>
                <Field label={token}>
                  <TextInput
                    type="password"
                    placeholder={token}
                    value={channel.token}
                    onChange={(event) => updateChannel(id, { token: event.currentTarget.value })}
                  />
                </Field>
                <div className="flex justify-end">
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={!channel.endpoint.trim() || testingChannelId === id}
                    onPress={() => void testChannel(id, label, channel)}
                  >
                    {testingChannelId === id
                      ? tm("settings.ai.provider.test.running", "Testing...")
                      : tm("settings.ai.provider.test", "Test")}
                  </Button>
                </div>
              </div>
            )}
          </SettingsCard>
        );
      })}
    </SettingsForm>
  );
}

function RemoteSection() {
  const [settings, setSettings] = useSyncedSettings();
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [isPairingSheetOpen, setPairingSheetOpen] = useState(false);
  const [activePendingPairing, setActivePendingPairing] = useState<RemotePendingPairing | null>(null);
  const [remoteAction, setRemoteAction] = useState<string | null>(null);
  const remote = settings.remote;
  const setRemote = (patch: Partial<typeof remote>) => {
    const next = updateAppSettings({
      remote: {
        ...remote,
        ...patch,
      },
    });
    setSettings(next);
  };
  const serverUrl = remote.serverURL.trim();
  const isConfigured = Boolean(serverUrl);
  const devices = status ? status.deviceList : remote.cachedDevices.filter((device) => !device.revokedAt);
  const statusLabel = remoteStatusLabel(remote.isEnabled, isConfigured, status);
  const statusDotClass = remoteStatusDotClass(remote.isEnabled, isConfigured, status?.status);
  const canCreatePairing = remote.isEnabled && isConfigured && status?.status === "connected";

  useEffect(() => {
    if (!status?.pairing) {
      setPairingSheetOpen(false);
    }
  }, [status?.pairing]);

  useEffect(() => {
    const pending = status?.pendingPairings?.[0] ?? null;
    setActivePendingPairing(pending);
    if (pending) {
      setPairingSheetOpen(false);
    }
  }, [status?.pendingPairings]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<RemoteStatus>("remote:status", (event) => {
      if (!disposed) setStatus(event.payload);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
      void invoke("remote_snapshot_emit").catch(() => undefined);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const runRemoteCommand = async (
    command:
      | "remote_reconnect"
      | "remote_devices_refresh"
      | "remote_device_revoke"
      | "remote_pairing_create"
      | "remote_pairing_confirm"
      | "remote_pairing_reject",
    args?: Record<string, unknown>,
  ) => {
    if (!window.__TAURI_INTERNALS__) return;
    setRemoteAction(command);
    await flushAppSettings().catch((error) => {
      console.error("failed to save remote settings before command", error);
    });
    try {
      const next = await invoke<RemoteStatus>(command, args);
      setStatus(next);
      return next;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      await systemMessage(message, {
        title: tm("settings.tab.remote", "Remote"),
        kind: "error",
      });
      throw error;
    } finally {
      setRemoteAction(null);
    }
  };
  const createPairing = async () => {
    setPairingSheetOpen(true);
    try {
      await runRemoteCommand("remote_pairing_create");
    } catch {
      setPairingSheetOpen(false);
    }
  };
  const closePairingSheet = () => {
    const pairingId = status?.pairing?.pairingId;
    setPairingSheetOpen(false);
    if (pairingId) {
      void runRemoteCommand("remote_pairing_reject", { pairingId });
    }
  };
  const decidePendingPairing = async (action: "confirm" | "reject", pairing: RemotePendingPairing) => {
    try {
      await runRemoteCommand(action === "confirm" ? "remote_pairing_confirm" : "remote_pairing_reject", {
        pairingId: pairing.id,
      });
      setActivePendingPairing(null);
    } catch {
      setActivePendingPairing(pairing);
    }
  };
  const revokeDevice = async (device: RemoteDeviceSettings) => {
    await runRemoteCommand("remote_device_revoke", { deviceId: device.id });
  };

  return (
    <>
      <SettingsForm className="max-w-[640px]">
        <SettingsCard title={tm("settings.remote.server", "Server")}>
          <Field label={tm("settings.remote.server_url", "Relay Server URL")}>
            <TextInput
              value={serverUrl}
              placeholder="https://relay.example.com"
              onChange={(event) => setRemote({ serverURL: event.currentTarget.value })}
            />
          </Field>
          <FormRow label={tm("settings.remote.enabled", "Enable Remote Host")}>
            <Toggle checked={remote.isEnabled} onChange={(isEnabled) => setRemote({ isEnabled })} />
          </FormRow>
          <div className="flex items-center gap-2 text-xs text-ink-faint">
            <span className={`h-2 w-2 rounded-full ${statusDotClass}`} />
            <span className="min-w-0 truncate">{statusLabel}</span>
            <span className="flex-1" />
            <Button
              variant="secondary"
              size="sm"
              disabled={!isConfigured}
              onPress={() => void runRemoteCommand("remote_reconnect")}
            >
              {tm("settings.remote.reconnect", "Reconnect")}
            </Button>
          </div>
        </SettingsCard>

        <SettingsCard
          title={tm("settings.remote.devices", "Devices")}
          action={
            <div className="flex items-center gap-1.5">
              <Button
                variant="ghost"
                size="sm"
                isIconOnly
                disabled={!canCreatePairing || remoteAction === "remote_pairing_create"}
                aria-label={tm("settings.remote.create_pairing", "Create Pairing QR")}
                leading={QrCode}
                onPress={() => void createPairing()}
              />
              <Button
                variant="ghost"
                size="sm"
                isIconOnly
                disabled={!remote.isEnabled || !isConfigured || remoteAction === "remote_devices_refresh"}
                aria-label={tm("settings.remote.refresh_devices", "Refresh Devices")}
                leading={RefreshCw}
                onPress={() => void runRemoteCommand("remote_devices_refresh")}
              />
            </div>
          }
        >
          {remote.isEnabled && isConfigured ? (
            <div className="grid gap-3">
              {devices.length > 0 ? (
                <div className="grid gap-2">
                  {devices.map((device) => (
                    <RemoteDeviceRow
                      key={device.id}
                      device={device}
                      isRemoving={remoteAction === "remote_device_revoke"}
                      onRemove={() => void revokeDevice(device)}
                    />
                  ))}
                </div>
              ) : (
                <div className="text-sm text-ink-mute">{tm("settings.remote.no_devices", "No paired devices.")}</div>
              )}
            </div>
          ) : (
            <div className="text-sm text-ink-faint">
              {tm("remote.devices.empty_hint", "Pair a phone to control terminals on the go.")}
            </div>
          )}
        </SettingsCard>
      </SettingsForm>

      <PairingQRCodeModal isOpen={isPairingSheetOpen} pairing={status?.pairing ?? null} onClose={closePairingSheet} />
      <PendingPairingModal
        pairing={activePendingPairing}
        onReject={(pairing) => void decidePendingPairing("reject", pairing)}
        onConfirm={(pairing) => void decidePendingPairing("confirm", pairing)}
      />
    </>
  );
}

function RemoteDeviceRow({
  device,
  isRemoving,
  onRemove,
}: {
  device: RemoteDeviceSettings;
  isRemoving: boolean;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md bg-fill/[0.035] px-2.5 py-2 text-sm">
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-ink">{device.name || tm("settings.remote.device", "Device")}</div>
        <div className="truncate text-xs text-ink-faint">{device.id}</div>
      </div>
      <div className="flex flex-none items-center gap-2">
        <span
          className={`rounded-full px-2 py-0.5 text-[11px] font-semibold ${device.online ? "bg-brand-green/12 text-brand-green" : "bg-fill/10 text-ink-faint"}`}
        >
          {device.online ? tm("remote.devices.online", "Online") : tm("remote.devices.offline", "Offline")}
        </span>
        <Button
          variant="ghost"
          size="sm"
          isIconOnly
          disabled={isRemoving}
          aria-label={tm("settings.remote.revoke", "Remove")}
          leading={Trash}
          onPress={onRemove}
        />
      </div>
    </div>
  );
}

function PairingQRCodeModal({
  isOpen,
  pairing,
  onClose,
}: {
  isOpen: boolean;
  pairing: RemoteStatus["pairing"] | null;
  onClose: () => void;
}) {
  const [qrDataUrl, setQrDataUrl] = useState("");

  useEffect(() => {
    let disposed = false;
    setQrDataUrl("");
    if (!isOpen || !pairing?.qrPayload) return;
    void QRCode.toDataURL(pairing.qrPayload, {
      errorCorrectionLevel: "M",
      margin: 1,
      scale: 8,
      color: { dark: "#111827", light: "#ffffff" },
    })
      .then((url) => {
        if (!disposed) setQrDataUrl(url);
      })
      .catch((error) => {
        console.error("failed to generate pairing QR", error);
      });
    return () => {
      disposed = true;
    };
  }, [isOpen, pairing?.qrPayload]);

  if (!isOpen) return null;

  return (
    <div className="no-drag fixed inset-0 z-[9500] grid place-items-center bg-black/30 p-4 backdrop-blur-sm">
      <div className="w-[min(420px,calc(100vw-32px))] rounded-[16px] border border-line-strong bg-surface-chrome p-5 text-ink shadow-pop">
        <div className="flex items-center justify-between gap-3">
          <div className="text-lg font-semibold tracking-tight">{tm("settings.remote.pairing", "Pairing")}</div>
          <Button
            variant="ghost"
            size="sm"
            isIconOnly
            aria-label={tm("common.close", "Close")}
            leading={X}
            onPress={onClose}
          />
        </div>

        <div className="mt-6 grid justify-items-center">
          {pairing ? (
            <>
              <div className="grid h-[242px] w-[242px] place-items-center rounded-[14px] border border-line bg-white p-[10px]">
                {qrDataUrl ? (
                  <img src={qrDataUrl} alt={tm("settings.remote.pairing", "Pairing")} className="h-[220px] w-[220px]" />
                ) : (
                  <div className="h-[220px] w-[220px] animate-pulse rounded-md bg-black/10" />
                )}
              </div>
              <div className="mt-4 text-center">
                <div className="text-xs text-ink-mute">
                  {tm("settings.remote.waiting_scan", "Waiting for mobile scan...")}
                </div>
                <div className="mt-1 text-[11px] text-ink-faint">{tm("settings.remote.scan_code", "Scan code")}</div>
                <div className="mt-1.5 font-mono text-[20px] font-semibold tracking-[0.18em] text-ink">
                  {pairing.code}
                </div>
              </div>
            </>
          ) : (
            <div className="grid h-[280px] place-items-center text-sm text-ink-mute">
              {tm("settings.remote.creating_pairing", "Creating pairing QR...")}
            </div>
          )}
        </div>

        <div className="mt-6 flex justify-center">
          <Button variant="secondary" onPress={onClose}>
            {tm("common.close", "Close")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function PendingPairingModal({
  pairing,
  onReject,
  onConfirm,
}: {
  pairing: RemotePendingPairing | null;
  onReject: (pairing: RemotePendingPairing) => void;
  onConfirm: (pairing: RemotePendingPairing) => void;
}) {
  if (!pairing) return null;
  const code = pairing.code || "-";
  return (
    <div className="no-drag fixed inset-0 z-[9600] grid place-items-center bg-black/30 p-4 backdrop-blur-sm">
      <div className="w-[min(380px,calc(100vw-32px))] rounded-[16px] border border-line-strong bg-surface-chrome p-5 text-ink shadow-pop">
        <div className="flex items-start gap-3">
          <div className="grid h-10 w-10 flex-none place-items-center rounded-full bg-brand-blue/14 text-brand-blue">
            <ShieldCheck size={20} strokeWidth={2} />
          </div>
          <div className="min-w-0">
            <div className="text-base font-semibold">
              {tm("settings.remote.confirm_pairing_title", "Confirm Device Pairing")}
            </div>
            <div className="mt-2 text-sm text-ink-mute">
              {formatI18n(
                tm("settings.remote.device_format", "Device: %@"),
                pairing.deviceName || tm("settings.remote.device", "Device"),
              )}
            </div>
            <div className="mt-1 text-sm text-ink-mute">
              {formatI18n(tm("settings.remote.match_code_format", "Match code: %@"), code)}
            </div>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onPress={() => onReject(pairing)}>
            {tm("settings.remote.reject_pairing", "Reject")}
          </Button>
          <Button variant="primary" onPress={() => onConfirm(pairing)}>
            {tm("settings.remote.confirm_pairing", "Confirm")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function remoteStatusLabel(enabled: boolean, configured: boolean, status: RemoteStatus | null) {
  if (!configured) return tm("remote.status.not_configured", "Remote not configured");
  if (!enabled) return tm("remote.status.disabled", "Remote disabled");
  if (status?.message && status.status === "failed") return status.message;
  switch (status?.status) {
    case "connected":
      return tm("remote.status.connected_label", "Connected");
    case "registering":
    case "connecting":
      return tm("remote.status.connecting_label", "Connecting");
    case "failed":
      return tm("remote.status.failed_label", "Error");
    case "stopped":
      return tm("remote.status.stopped_short", "Remote stopped");
    default:
      return tm("remote.status.connecting_label", "Connecting");
  }
}

function remoteStatusDotClass(enabled: boolean, configured: boolean, status?: RemoteStatus["status"]) {
  if (!enabled || !configured || status === "stopped") return "bg-ink-faint";
  if (status === "connected") return "bg-brand-green";
  if (status === "failed") return "bg-brand-red";
  return "bg-brand-amber";
}

function ShortcutSection() {
  const [settings, setSettings] = useSyncedSettings();
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const setShortcut = useCallback(
    (id: string, value: string) => {
      const next = updateAppSettings({
        shortcuts: {
          ...settings.shortcuts,
          [id]: value,
        },
      });
      setSettings(next);
    },
    [setSettings, settings.shortcuts],
  );
  const resetShortcut = (id: string) => {
    const shortcuts = { ...settings.shortcuts };
    delete shortcuts[id];
    const next = updateAppSettings({ shortcuts });
    setSettings(next);
  };
  const editShortcut = (id: string) => setRecordingId(id);

  useEffect(() => {
    if (!recordingId) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecordingId(null);
        return;
      }
      if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) return;
      const shortcut = {
        key: event.key.length === 1 ? event.key.toLowerCase() : event.key,
        meta: event.metaKey,
        ctrl: event.ctrlKey,
        alt: event.altKey,
        shift: event.shiftKey,
      };
      setShortcut(recordingId, serializeShortcutSequence([shortcut]));
      setRecordingId(null);
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [recordingId, setShortcut]);

  return (
    <SettingsForm className="max-w-[640px]">
      <SettingsCard title={tm("settings.tab.shortcuts", "Shortcuts")}>
        {appShortcutDefinitions.map((shortcut) => (
          <ShortcutRow
            key={shortcut.id}
            label={tm(shortcut.labelKey, shortcut.label)}
            value={
              recordingId === shortcut.id
                ? tm("settings.shortcut.record", "Record Shortcut")
                : (settings.shortcuts[shortcut.id] ?? shortcutDisplayValue(shortcut.id))
            }
            recording={recordingId === shortcut.id}
            customized={Boolean(settings.shortcuts[shortcut.id])}
            onEdit={() => editShortcut(shortcut.id)}
            onReset={() => resetShortcut(shortcut.id)}
          />
        ))}
      </SettingsCard>
      <SettingsCard title={tm("settings.shortcut.project_switch", "Project Switch Shortcuts")}>
        <div className="text-xs text-ink-faint">
          {tm("settings.shortcut.project_switch_hint", "Use ⌘1-⌘9 to switch projects in sidebar order.")}
        </div>
      </SettingsCard>
    </SettingsForm>
  );
}

function ExperimentSection() {
  const [agentSplit, setAgentSplit] = useState(false);

  return (
    <SettingsForm className="max-w-[640px]">
      <SettingsCard title={tm("settings.experiments.section.split", "Split Panes")}>
        <FormRow
          label={tm("settings.experiments.agent_split", "Agent Split")}
          description={tm(
            "settings.experiments.agent_split.help",
            "When enabled, creating a split lets you choose Terminal or Agent. When disabled, splits are created as normal terminal panes.",
          )}
        >
          <Toggle checked={agentSplit} onChange={setAgentSplit} />
        </FormRow>
      </SettingsCard>
    </SettingsForm>
  );
}

function DeveloperSection() {
  const [settings, setSettings] = useSyncedSettings();
  const setSetting = <K extends keyof typeof settings>(key: K, value: (typeof settings)[K]) => {
    const next = updateAppSettings({ [key]: value });
    setSettings(next);
  };

  return (
    <SettingsForm className="max-w-[640px]">
      <SettingsCard>
        <FormRow label={tm("settings.developer.performance_monitor", "Performance Monitor HUD")}>
          <Toggle checked={settings.developerHud} onChange={(value) => setSetting("developerHud", value)} />
        </FormRow>
        <Field label={tm("settings.developer.performance_monitor_interval", "Performance Monitor Interval")}>
          <Select
            value={settings.developerRefresh}
            onChange={(value) => setSetting("developerRefresh", value)}
            options={monitorRefreshOptions}
          />
        </Field>
      </SettingsCard>
    </SettingsForm>
  );
}

function ShortcutRow({
  label,
  value,
  recording,
  customized,
  onEdit,
  onReset,
}: {
  label: string;
  value: string;
  recording?: boolean;
  customized?: boolean;
  onEdit: () => void;
  onReset: () => void;
}) {
  return (
    <FormRow label={label}>
      <div className="flex items-center gap-2">
        <PressableButton
          onPressUp={onEdit}
          className={`h-7 min-w-[118px] rounded-md border px-2.5 text-sm font-semibold hover:text-ink ${
            recording
              ? "border-brand-blue/55 bg-brand-blue/12 text-brand-blue"
              : "border-line bg-fill/[0.055] text-ink-soft"
          }`}
        >
          {value}
        </PressableButton>
        {customized && (
          <Button variant="ghost" size="sm" onPress={onReset}>
            {tm("common.undo", "Undo")}
          </Button>
        )}
      </div>
    </FormRow>
  );
}

function ThemePreviewGrid({
  title,
  presets,
  selected,
  onSelect,
}: {
  title?: string;
  presets: { value: string; label: string; labelKey: string }[];
  selected: string;
  onSelect: (value: string) => void;
}) {
  return (
    <div className="grid gap-1.5">
      {title !== undefined && <div className="px-1 text-xs font-medium text-ink-faint">{title}</div>}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(96px,1fr))] gap-2">
        {presets.map((preset) => (
          <ThemePreviewButton
            key={preset.value}
            label={tm(preset.labelKey, preset.label)}
            value={preset.value}
            selected={selected === preset.value}
            onPress={() => onSelect(preset.value)}
          />
        ))}
      </div>
    </div>
  );
}

function ThemePreviewButton({
  label,
  value,
  selected,
  onPress,
}: {
  label: string;
  value: string;
  selected: boolean;
  onPress: () => void;
}) {
  const preview = terminalThemePreview(value);
  return (
    <PressableButton onPressUp={onPress} className="group min-w-0 text-center text-xs text-ink-mute outline-none">
      <span
        className={`block h-[46px] rounded-md border transition-colors ${
          selected ? "border-brand-blue ring-1 ring-brand-blue/40" : "border-line/70 group-hover:border-line-strong"
        }`}
        style={{ background: preview.background }}
      >
        <span className="block p-2 text-left">
          <span className="mb-1 block h-[3px] w-4 rounded-full" style={{ background: preview.mutedForeground }} />
          <span className="mb-1 block h-[3px] w-8 rounded-sm" style={{ background: preview.foreground }} />
          <span className="block h-[3px] w-6 rounded-sm" style={{ background: preview.mutedForeground }} />
          <span className="mt-2 block h-[7px] w-12 rounded-sm" style={{ background: preview.selection }} />
        </span>
      </span>
      <span className={`mt-1.5 block truncate ${selected ? "font-semibold text-ink" : ""}`}>{label}</span>
    </PressableButton>
  );
}

function ColorSwatchButton({
  label,
  color,
  selected,
  onPress,
}: {
  label: string;
  color: string;
  selected: boolean;
  onPress: () => void;
}) {
  const isAutomatic = label === "Auto";
  return (
    <PressableButton
      onPressUp={onPress}
      className="group grid justify-items-center gap-1 text-center text-[10px] text-ink-mute outline-none"
    >
      <span
        className={`grid h-7 w-7 place-items-center rounded-full border transition-colors ${
          selected ? "border-brand-blue ring-2 ring-brand-blue/25" : "border-line/80 group-hover:border-line-strong"
        }`}
        style={{ background: color }}
      >
        {isAutomatic && (
          <span className="text-[11px] font-bold text-white [text-shadow:0_1px_2px_rgb(0_0_0/.35)]">A</span>
        )}
      </span>
      <span className={`block w-11 truncate ${selected ? "font-semibold text-ink" : ""}`}>{label}</span>
    </PressableButton>
  );
}

function AppIconPreviewButton({
  label,
  styleName,
  selected,
  onPress,
}: {
  label: string;
  styleName: string;
  selected: boolean;
  onPress: () => void;
}) {
  return (
    <PressableButton
      onPressUp={onPress}
      className="group grid w-[72px] justify-items-center gap-1.5 text-center text-xs text-ink-mute outline-none"
    >
      <span
        className={`grid h-12 w-12 place-items-center rounded-[12px] transition-colors ${
          selected ? "ring-2 ring-brand-blue ring-offset-2 ring-offset-surface-chrome" : ""
        }`}
      >
        <AppIconMark styleName={styleName} size={48} />
      </span>
      <span className={`block w-full truncate ${selected ? "font-semibold text-ink" : "group-hover:text-ink"}`}>
        {label}
      </span>
    </PressableButton>
  );
}

function intervalOptions(seconds: number[]) {
  return seconds.map((value) => {
    const label = value % 60 === 0 ? `${value / 60} min` : `${value} sec`;
    return { value: String(value), label };
  });
}
