import {
  Bot,
  KeyRound,
  Palette,
  Radio,
  Server,
  Settings,
  Smile,
  Sparkles,
  Wrench,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import { checkForUpdates, type UpdateInstallResult, type UpdateStatus } from "../appActions";
import { Button } from "../components/Button";
import { PressableButton } from "../components/PressableButton";
import {
  Field,
  SettingsCard,
  FormRow,
  Select,
  SettingsForm,
  Textarea,
  TextInput,
  Toggle,
} from "../components/Form";
import { closeCurrentAppWindow, revealCurrentAppWindow } from "../windowing";
import type { AppIcon } from "../icons";
import {
  readAppSettings,
  subscribeAppSettings,
  syncAppSettingsFromRust,
  updateAppSettings,
  type AppSettings,
  type AIProviderSettings,
  type NotificationChannelSettings,
} from "../settings";
import {
  appShortcutDefinitions,
  serializeShortcutSequence,
  shortcutDisplayValue,
} from "../shortcuts";
import { restartNotice, systemMessage } from "../systemDialog";
import { backgroundColorOptions, systemThemeOptions, terminalThemeOptions, terminalThemePreview } from "../theme";
import { formatI18n, tm } from "../i18n";

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
  { id: "general", labelKey: "settings.tab.general", label: "General", description: "settings.tab.general.description", icon: Settings },
  { id: "appearance", labelKey: "settings.tab.appearance", label: "Appearance", description: "settings.tab.appearance.description", icon: Palette },
  { id: "pet", labelKey: "settings.tab.pet", label: "Pet", description: "settings.tab.pet.description", icon: Smile },
  { id: "ai", labelKey: "settings.tab.ai", label: "AI", description: "settings.tab.ai.description", icon: Bot },
  { id: "notifications", labelKey: "settings.tab.notifications", label: "Notifications", description: "settings.tab.notifications.description", icon: Radio },
  { id: "remote", labelKey: "settings.tab.remote", label: "Remote", description: "settings.tab.remote.description", icon: Server },
  { id: "shortcuts", labelKey: "settings.tab.shortcuts", label: "Shortcuts", description: "settings.tab.shortcuts.description", icon: KeyRound },
  { id: "experiments", labelKey: "settings.tab.experiments", label: "Experiments", description: "settings.tab.experiments.description", icon: Sparkles },
  { id: "developer", labelKey: "settings.tab.developer", label: "Developer", description: "settings.tab.developer.description", icon: Wrench },
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

const shellOptions = [
  { value: "system", label: tm("settings.default_shell.system", "Follow System") },
  ...["zsh", "bash", "sh", "fish", "powershell.exe", "cmd.exe"].map((value) => ({ value, label: value })),
];
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
  { id: "codex", label: "Codex", model: "gpt-5.5" },
  { id: "claudeCode", label: "Claude Code", model: "claude-sonnet-4.5" },
  { id: "gemini", label: "Gemini", model: "gemini-2.5-pro" },
  { id: "opencode", label: "OpenCode", model: "gpt-5.5" },
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

const appIconStyles = [
  { value: "default", labelKey: "settings.app_icon.option.default", label: "Default", top: "#3d80fa", bottom: "#295cdb" },
  { value: "cobalt", labelKey: "settings.app_icon.option.cobalt", label: "Cobalt", top: "#1f2433", bottom: "#141722" },
  { value: "sunset", labelKey: "settings.app_icon.option.sunset", label: "Sunset", top: "#f56b52", bottom: "#e04c42" },
  { value: "forest", labelKey: "settings.app_icon.option.forest", label: "Forest", top: "#2fa46f", bottom: "#146646" },
];

const darkTerminalThemes = terminalThemeOptions.filter((preset) => {
  if (preset.value === "Auto") return false;
  return terminalThemePreview(preset.value).appTheme !== "light";
});
const lightTerminalThemes = terminalThemeOptions.filter((preset) => terminalThemePreview(preset.value).appTheme === "light");

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

  useEffect(() => {
    void revealCurrentAppWindow();
  }, []);

  const dismiss = () => {
    void closeCurrentAppWindow();
  };

  return (
    <div className="h-screen grid grid-cols-[200px_minmax(0,1fr)] bg-surface-chrome/92 text-ink">
      <aside className="min-h-0 border-r border-line bg-fill/[0.02] flex flex-col">
        <div className="h-14 flex-shrink-0 drag-region" data-tauri-drag-region />
        <nav className="min-h-0 flex-1 overflow-y-auto px-3 pb-3 no-drag">
          <div className="grid gap-1.5">
            {sections.map((item) => (
              <PressableButton
                key={item.id}
                onPressUp={() => setActive(item.id)}
                className={`h-8 px-2.5 rounded-md grid grid-cols-[18px_minmax(0,1fr)] items-center gap-2 text-sm text-left transition-colors ${
                  active === item.id
                    ? "bg-brand-blue/16 text-ink"
                    : "text-ink-soft hover:bg-fill/[0.06] hover:text-ink"
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
          className="absolute left-0 right-0 top-0 z-20 h-[92px] flex flex-col justify-end px-6 pb-4 drag-region"
          data-tauri-drag-region
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

        <main className="absolute inset-0 overflow-y-auto px-5 pt-[108px] pb-[78px] no-drag">
          <SettingsPane active={active} />
        </main>

        <footer
          className="absolute left-0 right-0 bottom-0 z-20 h-[58px] px-5 flex items-center justify-end gap-2 no-drag"
          style={{
            background: "linear-gradient(to bottom, transparent 0%, var(--color-surface-chrome) 50%)",
          }}
        >
          <Button variant="ghost" onPress={dismiss}>
            {tm("common.cancel", "Cancel")}
          </Button>
          <Button variant="primary" onPress={dismiss}>
            {tm("common.save", "Save")}
          </Button>
        </footer>
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
  const refreshUpdateStatus = async () => {
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
  };
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
  }, [settings.update.enabled, settings.update.channel, settings.update.endpoint]);

  return (
    <SettingsForm className="max-w-[640px]">
      <SettingsCard>
        <Field label={tm("settings.language", "Language")}>
          <Select value={settings.language} onChange={(value) => setSetting("language", value)} options={languageOptions} />
        </Field>
        <Field label={tm("settings.default_shell", "Default Shell")}>
          <Select value={settings.shell} onChange={(value) => setSetting("shell", value)} options={shellOptions} />
        </Field>
        <FormRow label={tm("settings.dock_badge", "Dock Badge")}>
          <Toggle checked={settings.showsDockBadge} onChange={(value) => setSetting("showsDockBadge", value)} />
        </FormRow>
        <Field
          label={tm("settings.sleep_prevention", "Prevent System Sleep")}
          description={tm("settings.sleep_prevention.help", "Allows the display to turn off, but prevents this device from idle sleeping while enabled.")}
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
          <Select value={settings.gitRefresh} onChange={(value) => setSetting("gitRefresh", value)} options={gitRefreshOptions} />
        </Field>
        <Field label={tm("settings.ai_auto_refresh", "AI Auto Refresh")}>
          <Select value={settings.aiRefresh} onChange={(value) => setSetting("aiRefresh", value)} options={aiRefreshOptions} />
        </Field>
        <Field label={tm("settings.ai_background_refresh", "AI Background Refresh")}>
          <Select
            value={settings.aiBackgroundRefresh}
            onChange={(value) => setSetting("aiBackgroundRefresh", value)}
            options={aiBackgroundRefreshOptions}
          />
        </Field>
        <Field label={tm("settings.ai_statistics_mode", "AI Statistics Mode")}>
          <Select value={settings.statisticsMode} onChange={(value) => setSetting("statisticsMode", value)} options={statisticsModeOptions} />
        </Field>
      </SettingsCard>

      <SettingsCard
        title={tm("settings.update.section", "Updates")}
        description={tm("settings.update.description", "Updates use signed Tauri artifacts from the tauri-stable GitHub Release.")}
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
        <Field
          label={tm("settings.update.endpoint", "Update Endpoint")}
          description={tm("settings.update.endpoint_help", "The default endpoint reads the signed latest.json asset without changing the native macOS release channel.")}
        >
          <TextInput
            value={settings.update.endpoint}
            placeholder="https://github.com/duxweb/codux/releases/download/tauri-stable/latest.json"
            onChange={(event) => setSetting("update", { ...settings.update, endpoint: event.currentTarget.value })}
          />
        </Field>
        <FormRow
          label={tm("settings.update.status", "Update Status")}
          description={updateStatus?.message ?? tm("update.checking", "Checking for updates...")}
        >
          <div className="flex items-center gap-2">
            <span className="rounded-md border border-line bg-fill/[0.04] px-2 py-1 text-xs font-medium text-ink-faint">
              {updateModeLabel(updateStatus)}
            </span>
            <Button
              size="sm"
              variant="secondary"
              disabled={isCheckingUpdates}
              onPress={() => void checkForUpdates().then(refreshUpdateStatus)}
            >
              {isCheckingUpdates ? tm("update.checking", "Checking...") : tm("about.updates", "Check for Updates")}
            </Button>
            {updateStatus?.available && updateStatus.automaticInstallSupported && (
              <Button
                size="sm"
                variant="primary"
                disabled={isInstallingUpdate}
                onPress={() => void installUpdate()}
              >
                {isInstallingUpdate ? tm("update.installing", "Installing...") : tm("update.available.install", "Install")}
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
  if (status.automaticInstallSupported) {
    return tm("settings.update.mode.automatic", "Automatic");
  }
  if (status.manifestEndpointConfigured) {
    return tm("settings.update.mode.manual_manifest", "Manual");
  }
  if (status.signedUpdaterConfigured) {
    return tm("settings.update.mode.signed_config", "Signed Config");
  }
  if (!status.configured) {
    return tm("settings.update.mode.not_configured", "Not Configured");
  }
  return status.installationMode;
}

function AppearanceSection() {
  const [settings, setSettings] = useSyncedSettings();
  const setSetting = <K extends keyof typeof settings>(key: K, value: (typeof settings)[K]) => {
    const next = updateAppSettings({ [key]: value });
    setSettings(next);
    if (key === "theme" || key === "background") {
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
        description={tm("settings.terminal_theme.restart_pending", "Terminal and editor theme changes will apply after restart.")}
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
        description={tm("settings.background_color.help", "Tints the app window background while preserving the translucent desktop material.")}
      >
        <div className="grid grid-cols-[repeat(auto-fill,minmax(48px,1fr))] gap-x-2 gap-y-3 py-1">
          {backgroundColorOptions.map((item) => (
            <ColorSwatchButton
              key={item.label}
              label={item.label}
              color={item.color}
              selected={settings.background === item.label}
              onPress={() => setSetting("background", item.label)}
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
              top={style.top}
              bottom={style.bottom}
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
        <Field
          label={tm("settings.pet.speech.frequency", "Frequency")}
          description={tm("settings.pet.speech.frequency_help", "Frequency is estimated per hour, not a daily cap. The shortest global cooldown is 30 seconds.")}
        >
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
            onChange={(enabled) => setAIPet({
              speechQuietHoursStart: enabled ? 22 : null,
              speechQuietHoursEnd: enabled ? 8 : null,
            })}
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
        description={tm("settings.pet.llm.help", "Only rhythm and milestone messages try LLM polishing. Template lines are used if it fails, times out, or no LLM channel is available.")}
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
        <FormRow
          label={tm("settings.pet.reminder.hydration", "Hydration Reminder")}
        >
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
  const setMemory = (patch: Partial<typeof ai.memory>) => {
    setAI({ memory: { ...ai.memory, ...patch } });
  };
  const upsertProvider = (id: string, patch: Partial<AIProviderSettings>) => {
    setAI({
      providers: providers.map((provider) =>
        provider.id === id ? { ...provider, ...patch } : provider,
      ),
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
      <SettingsCard
        title={tm("settings.ai.section.runtime_tools", "Runtime Tools")}
        description={tm("settings.tools.hint", "These defaults apply the next time a supported AI tool is launched inside a Codux terminal. Explicit command-line flags still take priority.")}
      >
        {runtimeTools.map((tool) => (
          <ReadOnlySetting
            key={tool.id}
            label={formatI18n(tm("settings.ai.tool.configuration_format", "%@ Configuration"), tool.label)}
            value={tool.model}
          />
        ))}
      </SettingsCard>

      <SettingsCard
        title={tm("settings.ai.global_prompt", "Global Prompt")}
        description={tm("settings.ai.global_prompt_help", "Injected when supported tools start. It is merged with memory context and written to each tool's launch context.")}
      >
        <Textarea
          value={ai.globalPrompt}
          rows={4}
          onChange={(event) => setAI({ globalPrompt: event.currentTarget.value })}
        />
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
          <Field
            label={tm("settings.ai.memory.default_extraction_provider", "Default Extraction Provider")}
            description={tm("settings.ai.memory.extraction_provider.automatic_help", "Automatic uses selected memory providers by priority.")}
          >
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
        description={tm("settings.ai.provider.api_only_help", "Local and API providers for memory extraction.")}
      >
        <div className="flex justify-end">
          <Button size="sm" variant="secondary" onPress={() => addProvider("openAICompatible")}>
            {tm("settings.ai.provider.add", "Add API Channel")}
          </Button>
        </div>
        {providers.length === 0 && (
          <div className="text-sm text-ink-faint">
            {tm("settings.ai.provider.empty", "No API channel configured.")}
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
          <Toggle
            checked={provider.isEnabled}
            onChange={(isEnabled) => onChange({ isEnabled })}
          />
          <Button size="sm" variant="ghost" onPress={onRemove}>
            {tm("settings.ai.provider.remove", "Remove")}
          </Button>
        </div>
      </div>
      <Field label={tm("settings.ai.provider.kind", "Kind")}>
        <Select
          value={provider.kind}
          onChange={changeKind}
          options={aiProviderKindOptions}
        />
      </Field>
      <Field label={tm("settings.ai.provider.name", "Name")}>
        <TextInput
          value={provider.displayName}
          onChange={(event) => onChange({ displayName: event.currentTarget.value })}
        />
      </Field>
      <Field label={tm("settings.ai.provider.model", "Model")}>
        <TextInput
          value={provider.model}
          onChange={(event) => onChange({ model: event.currentTarget.value })}
        />
      </Field>
      <Field label={tm("settings.ai.provider.base_url", "Base URL")}>
        <TextInput
          value={provider.baseUrl}
          onChange={(event) => onChange({ baseUrl: event.currentTarget.value })}
        />
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
          {testing
            ? tm("settings.ai.provider.test.running", "Testing...")
            : tm("settings.ai.provider.test", "Test")}
        </Button>
      </div>
    </div>
  );
}

function aiProviderOptions(providers: AIProviderSettings[], purpose: "memory" | "petSpeech") {
  return [
    { value: "automatic", label: tm("settings.ai.memory.extraction_provider.automatic", "Automatic") },
    ...providers
      .filter((provider) =>
        provider.isEnabled &&
        (purpose !== "memory" || provider.useForMemoryExtraction) &&
        provider.kind !== "localLlama"
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
        formatI18n(tm("settings.ai.provider.test.failed_format", "Test failed: %@"), error instanceof Error ? error.message : String(error)),
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
                <Toggle
                  checked={channel.enabled}
                  onChange={(enabled) => updateChannel(id, { enabled })}
                />
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
  const isConfigured = Boolean(remote.relayUrl.trim());

  return (
    <SettingsForm className="max-w-[640px]">
      <SettingsCard title={tm("settings.remote.server", "Server")}>
        <Field label={tm("settings.remote.server_url", "Relay Server URL")}>
          <TextInput
            value={remote.relayUrl}
            placeholder="https://relay.example.com"
            onChange={(event) => setRemote({ relayUrl: event.currentTarget.value })}
          />
        </Field>
        <FormRow label={tm("settings.remote.enabled", "Enable Remote Host")}>
          <Toggle checked={remote.enabled} onChange={(enabled) => setRemote({ enabled })} />
        </FormRow>
        <div className="flex items-center gap-2 text-xs text-ink-faint">
          <span
            className={`h-2 w-2 rounded-full ${remote.enabled && isConfigured ? "bg-brand-amber" : "bg-ink-faint"}`}
          />
          <span>
            {remote.enabled && isConfigured
              ? tm("remote.status.connecting_label", "Connecting")
              : remote.enabled
                ? tm("remote.status.not_configured", "Remote not configured")
                : tm("remote.status.disabled", "Remote disabled")}
          </span>
          <span className="flex-1" />
          <Button variant="secondary" size="sm" disabled={!isConfigured}>
            {tm("settings.remote.reconnect", "Reconnect")}
          </Button>
        </div>
      </SettingsCard>

      <SettingsCard title={tm("settings.remote.devices", "Devices")}>
        {remote.enabled && isConfigured ? (
          <div className="flex items-center justify-between gap-3 text-sm text-ink-mute">
            <span>{tm("remote.devices.empty", "No paired devices")}</span>
            <div className="flex gap-2">
              <Button variant="secondary" size="sm" disabled>{tm("settings.remote.create_pairing", "Create Pairing QR")}</Button>
              <Button variant="ghost" size="sm" disabled>{tm("common.refresh", "Refresh")}</Button>
            </div>
          </div>
        ) : (
          <div className="text-sm text-ink-faint">
            {tm("remote.devices.empty_hint", "Pair a phone to control terminals on the go.")}
          </div>
        )}
      </SettingsCard>
    </SettingsForm>
  );
}

function ShortcutSection() {
  const [settings, setSettings] = useSyncedSettings();
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const setShortcut = (id: string, value: string) => {
    const next = updateAppSettings({
      shortcuts: {
        ...settings.shortcuts,
        [id]: value,
      },
    });
    setSettings(next);
  };
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
            value={recordingId === shortcut.id ? tm("settings.shortcut.record", "Record Shortcut") : settings.shortcuts[shortcut.id] ?? shortcutDisplayValue(shortcut.id)}
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
          description={tm("settings.experiments.agent_split.help", "When enabled, creating a split lets you choose Terminal or Agent. When disabled, splits are created as normal terminal panes.")}
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

function ReadOnlySetting({ label, value }: { label: ReactNode; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <div className="min-w-0 text-sm font-medium text-ink">{label}</div>
      <div className="rounded-md border border-line bg-fill/[0.035] px-2 py-1 text-xs font-medium text-ink-faint">
        {value}
      </div>
    </div>
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
      {title !== undefined && (
        <div className="px-1 text-xs font-medium text-ink-faint">{title}</div>
      )}
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
    <PressableButton
      onPressUp={onPress}
      className="group min-w-0 text-center text-xs text-ink-mute outline-none"
    >
      <span
        className={`block h-[46px] rounded-md border transition-colors ${
          selected ? "border-brand-blue ring-1 ring-brand-blue/40" : "border-line/70 group-hover:border-line-strong"
        }`}
        style={{ background: preview.background }}
      >
        <span className="block p-2 text-left">
          <span
            className="mb-1 block h-[3px] w-4 rounded-full"
            style={{ background: preview.mutedForeground }}
          />
          <span
            className="mb-1 block h-[3px] w-8 rounded-sm"
            style={{ background: preview.foreground }}
          />
          <span
            className="block h-[3px] w-6 rounded-sm"
            style={{ background: preview.mutedForeground }}
          />
          <span
            className="mt-2 block h-[7px] w-12 rounded-sm"
            style={{ background: preview.selection }}
          />
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
        {isAutomatic && <span className="text-[11px] font-bold text-white [text-shadow:0_1px_2px_rgb(0_0_0/.35)]">A</span>}
      </span>
      <span className={`block w-11 truncate ${selected ? "font-semibold text-ink" : ""}`}>{label}</span>
    </PressableButton>
  );
}

function AppIconPreviewButton({
  label,
  top,
  bottom,
  selected,
  onPress,
}: {
  label: string;
  top: string;
  bottom: string;
  selected: boolean;
  onPress: () => void;
}) {
  return (
    <PressableButton
      onPressUp={onPress}
      className="group grid w-[72px] justify-items-center gap-1.5 text-center text-xs text-ink-mute outline-none"
    >
      <span
        className={`relative grid h-12 w-12 place-items-center overflow-hidden rounded-[13px] border transition-colors ${
          selected ? "border-brand-blue ring-2 ring-brand-blue/25" : "border-transparent"
        }`}
        style={{ background: `linear-gradient(180deg, ${top}, ${bottom})` }}
      >
        <span className="absolute inset-x-1 top-0 h-7 rounded-full bg-white/10 blur-[6px]" />
        <span className="absolute inset-x-1 bottom-0 h-6 rounded-full bg-black/10 blur-[5px]" />
        <span className="absolute left-[14px] top-[15px] h-[18px] w-[12px] border-r-[5px] border-t-[5px] border-white/40 rotate-45" />
        <span className="absolute left-[24px] top-[15px] h-[18px] w-[12px] border-r-[5px] border-t-[5px] border-white rotate-45 drop-shadow-[0_1px_1px_rgb(0_0_0/.22)]" />
      </span>
      <span className={`block w-full truncate ${selected ? "font-semibold text-ink" : "group-hover:text-ink"}`}>{label}</span>
    </PressableButton>
  );
}

function intervalOptions(seconds: number[]) {
  return seconds.map((value) => {
    const label = value % 60 === 0 ? `${value / 60} min` : `${value} sec`;
    return { value: String(value), label };
  });
}
