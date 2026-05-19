import { invoke } from "@tauri-apps/api/core";
import { readAppSettings, subscribeAppSettings, type AppSettings } from "./settings";

export type LocaleKey =
  | "workspace"
  | "projects"
  | "tasks"
  | "split"
  | "memory"
  | "remote"
  | "pet"
  | "aiAssistant"
  | "ssh"
  | "git"
  | "files"
  | "review"
  | "addProject"
  | "settings"
  | "help"
  | "terminal.copy"
  | "terminal.paste"
  | "terminal.clear"
  | "terminal.selectAll"
  | "terminal.split"
  | "terminal.newTab";

export type Locale =
  | "zh-Hans"
  | "zh-Hant"
  | "en"
  | "ja"
  | "ko"
  | "fr"
  | "de"
  | "es"
  | "pt-BR"
  | "ru";

type I18nBundle = {
  sourceLanguage: string;
  locales: string[];
  strings: Record<string, Record<string, string>>;
};

const FALLBACK_LOCALE: Locale = "en";

const fallbackDictionaries: Record<Locale, Partial<Record<LocaleKey, string>>> = {
  "zh-Hans": {
    workspace: "工作区",
    projects: "项目",
    tasks: "任务",
    split: "分屏",
    memory: "记忆",
    remote: "远程",
    pet: "宠物",
    aiAssistant: "AI 助手",
    ssh: "SSH",
    git: "Git",
    files: "文件",
    review: "评审",
    addProject: "添加项目",
    settings: "设置",
    help: "帮助",
    "terminal.copy": "复制",
    "terminal.paste": "粘贴",
    "terminal.clear": "清屏",
    "terminal.selectAll": "全选",
    "terminal.split": "新建分屏",
    "terminal.newTab": "新建标签页",
  },
  "zh-Hant": {
    workspace: "工作區",
    projects: "專案",
    tasks: "任務",
    split: "分割",
    memory: "記憶",
    remote: "遠端",
    pet: "寵物",
    aiAssistant: "AI 助手",
    ssh: "SSH",
    git: "Git",
    files: "檔案",
    review: "評審",
    addProject: "新增專案",
    settings: "設定",
    help: "說明",
    "terminal.copy": "複製",
    "terminal.paste": "貼上",
    "terminal.clear": "清屏",
    "terminal.selectAll": "全選",
    "terminal.split": "新增分割",
    "terminal.newTab": "新增標籤頁",
  },
  en: {
    workspace: "Workspace",
    projects: "Projects",
    tasks: "Tasks",
    split: "Split",
    memory: "Memory",
    remote: "Remote",
    pet: "Pet",
    aiAssistant: "AI Assistant",
    ssh: "SSH",
    git: "Git",
    files: "Files",
    review: "Review",
    addProject: "Add Project",
    settings: "Settings",
    help: "Help",
    "terminal.copy": "Copy",
    "terminal.paste": "Paste",
    "terminal.clear": "Clear",
    "terminal.selectAll": "Select All",
    "terminal.split": "New Split",
    "terminal.newTab": "New Tab",
  },
  ja: {},
  ko: {},
  fr: {},
  de: {},
  es: {},
  "pt-BR": {},
  ru: {},
};

const bundleKeyAliases: Partial<Record<LocaleKey, string>> = {
  projects: "titlebar.projects",
  split: "titlebar.split",
  pet: "pet.tooltip.pet",
  aiAssistant: "ai.panel.title",
  ssh: "titlebar.ssh",
  git: "titlebar.git",
  files: "titlebar.files",
  review: "titlebar.review",
  addProject: "sidebar.footer.add_project",
  settings: "menu.settings",
  help: "sidebar.footer.help",
  "terminal.copy": "common.copy",
  "terminal.paste": "common.paste",
  "terminal.clear": "common.clear_screen",
  "terminal.selectAll": "common.select_all",
  "terminal.split": "workspace.create_split.title",
};

let runtimeBundle: I18nBundle | null = null;
let i18nSyncPromise: Promise<void> | null = null;
let runtimeLocale: Locale | null = null;

export async function syncI18nBundleFromRust() {
  if (!window.__TAURI_INTERNALS__) return;
  i18nSyncPromise ??= invoke<I18nBundle>("i18n_bundle_get")
    .then((bundle) => {
      runtimeBundle = bundle;
    })
    .catch((error) => {
      console.error("failed to load i18n bundle", error);
    })
    .finally(() => {
      i18nSyncPromise = null;
    });
  await i18nSyncPromise;
}

export function localeFromSettings(settings: AppSettings = readAppSettings()): Locale {
  switch (settings.language) {
    case "english":
      return "en";
    case "simplifiedChinese":
      return "zh-Hans";
    case "traditionalChinese":
      return "zh-Hant";
    case "japanese":
      return "ja";
    case "korean":
      return "ko";
    case "french":
      return "fr";
    case "german":
      return "de";
    case "spanish":
      return "es";
    case "portugueseBrazil":
      return "pt-BR";
    case "russian":
      return "ru";
    default:
      return systemLocale();
  }
}

export function lockRuntimeLocale(settings: AppSettings = readAppSettings()) {
  runtimeLocale = localeFromSettings(settings);
}

export function t(key: LocaleKey, settings = readAppSettings()) {
  const fallback =
    fallbackDictionaries[runtimeLocale ?? localeFromSettings(settings)]?.[key] ??
    fallbackDictionaries.en[key] ??
    fallbackDictionaries["zh-Hans"][key] ??
    key;
  return tm(bundleKeyAliases[key] ?? key, fallback, settings);
}

export function tm(key: string, fallback?: string, settings = readAppSettings()) {
  const locale = runtimeLocale ?? localeFromSettings(settings);
  return (
    runtimeBundle?.strings[locale]?.[key] ??
    runtimeBundle?.strings.en?.[key] ??
    runtimeBundle?.strings["zh-Hans"]?.[key] ??
    fallback ??
    key
  );
}

export function formatI18n(template: string, ...values: Array<string | number>) {
  let index = 0;
  return template.replace(/%@|%d|%lld/g, () => String(values[index++] ?? ""));
}

export function subscribeLocale(listener: () => void) {
  return subscribeAppSettings(listener);
}

function systemLocale(): Locale {
  if (typeof navigator === "undefined") return FALLBACK_LOCALE;
  const language = navigator.language || navigator.languages?.[0] || "";
  const normalized = language.toLowerCase();
  if (normalized.startsWith("zh-tw") || normalized.startsWith("zh-hk") || normalized.startsWith("zh-mo")) {
    return "zh-Hant";
  }
  if (normalized.startsWith("zh")) return "zh-Hans";
  if (normalized.startsWith("ja")) return "ja";
  if (normalized.startsWith("ko")) return "ko";
  if (normalized.startsWith("fr")) return "fr";
  if (normalized.startsWith("de")) return "de";
  if (normalized.startsWith("es")) return "es";
  if (normalized.startsWith("pt-br")) return "pt-BR";
  if (normalized.startsWith("ru")) return "ru";
  if (normalized.startsWith("en")) return "en";
  return FALLBACK_LOCALE;
}
