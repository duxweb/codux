import { getCurrentWindow, type Theme } from "@tauri-apps/api/window";
import { readAppSettings, type AppSettings } from "./settings";

export type AppTheme = "light" | "dark" | "graphite" | "midnight";

type TerminalThemeProfile = {
  appTheme: AppTheme | "system";
  variables: Partial<Record<ManagedThemeVariable, string>>;
};

type ManagedThemeVariable =
  | "--terminal-bg"
  | "--terminal-fg"
  | "--terminal-cursor"
  | "--terminal-selection"
  | "--terminal-black"
  | "--terminal-red"
  | "--terminal-green"
  | "--terminal-yellow"
  | "--terminal-blue"
  | "--terminal-magenta"
  | "--terminal-cyan"
  | "--terminal-white"
  | "--terminal-bright-black"
  | "--terminal-bright-red"
  | "--terminal-bright-green"
  | "--terminal-bright-yellow"
  | "--terminal-bright-blue"
  | "--terminal-bright-magenta"
  | "--terminal-bright-cyan"
  | "--terminal-bright-white"
  | "--editor-comment"
  | "--editor-keyword"
  | "--editor-atom"
  | "--editor-string"
  | "--editor-string2"
  | "--editor-number"
  | "--editor-variable"
  | "--editor-variable2"
  | "--editor-type"
  | "--editor-class"
  | "--editor-property"
  | "--editor-operator"
  | "--editor-punctuation"
  | "--editor-meta"
  | "--editor-link"
  | "--editor-heading"
  | "--editor-inserted"
  | "--editor-deleted"
  | "--editor-invalid"
  | "--surface-window-tint"
  | "--surface-window-glass"
  | "--surface-editor"
  | "--color-brand-blue"
  | "--color-brand-blue-deep"
  | "--color-on-brand"
  | "--accent"
  | "--accent-hover"
  | "--accent-foreground"
  | "--accent-soft"
  | "--accent-soft-foreground"
  | "--focus"
  | "--link"
  | "--color-surface-glass"
  | "--color-surface-chrome"
  | "--color-surface-panel"
  | "--color-surface-card"
  | "--color-surface-terminal"
  | "--color-surface-editor";

export type TerminalThemeOption = {
  value: string;
  label: string;
  labelKey: string;
};

export type BackgroundColorOption = {
  label: string;
  dark: string;
  light: string;
  preview: string;
};

export type ThemeColorOption = {
  label: string;
  color: string;
};

export type TerminalThemePreview = {
  background: string;
  foreground: string;
  mutedForeground: string;
  selection: string;
  appTheme: AppTheme | "system";
};

export const terminalThemeOptions: TerminalThemeOption[] = [
  { value: "Auto", label: "Auto", labelKey: "settings.theme.auto" },
  {
    value: "Tokyo Night Storm",
    label: "TokyoNight Storm",
    labelKey: "settings.terminal_theme.preset.tokyonight_storm",
  },
  {
    value: "Tokyo Night Night",
    label: "TokyoNight Night",
    labelKey: "settings.terminal_theme.preset.tokyonight_night",
  },
  { value: "Catppuccin Mocha", label: "Catppuccin Mocha", labelKey: "settings.terminal_theme.preset.catppuccin_mocha" },
  { value: "Rose Pine Moon", label: "Rose Pine Moon", labelKey: "settings.terminal_theme.preset.rose_pine_moon" },
  { value: "Kanagawa Wave", label: "Kanagawa Wave", labelKey: "settings.terminal_theme.preset.kanagawa_wave" },
  { value: "Material Ocean", label: "Material Ocean", labelKey: "settings.terminal_theme.preset.material_ocean" },
  { value: "Ayu Mirage", label: "Ayu Mirage", labelKey: "settings.terminal_theme.preset.ayu_mirage" },
  { value: "Dracula", label: "Dracula", labelKey: "settings.terminal_theme.preset.dracula" },
  { value: "Dracula+", label: "Dracula+", labelKey: "settings.terminal_theme.preset.dracula_plus" },
  { value: "GitHub Dark", label: "GitHub Dark", labelKey: "settings.terminal_theme.preset.github_dark" },
  { value: "Gruvbox Dark", label: "Gruvbox Dark", labelKey: "settings.terminal_theme.preset.gruvbox_dark" },
  {
    value: "Gruvbox Material Dark",
    label: "Gruvbox Material Dark",
    labelKey: "settings.terminal_theme.preset.gruvbox_material_dark",
  },
  { value: "Nord", label: "Nord", labelKey: "settings.terminal_theme.preset.nord" },
  { value: "Tokyo Night Day", label: "TokyoNight Day", labelKey: "settings.terminal_theme.preset.tokyonight_day" },
  { value: "GitHub Light", label: "GitHub Light", labelKey: "settings.terminal_theme.preset.github_light" },
  { value: "Catppuccin Latte", label: "Catppuccin Latte", labelKey: "settings.terminal_theme.preset.catppuccin_latte" },
  { value: "Flexoki Light", label: "Flexoki Light", labelKey: "settings.terminal_theme.preset.flexoki_light" },
  { value: "Flexoki Dark", label: "Flexoki Dark", labelKey: "settings.terminal_theme.preset.flexoki_dark" },
  { value: "Gruvbox Light", label: "Gruvbox Light", labelKey: "settings.terminal_theme.preset.gruvbox_light" },
  {
    value: "Gruvbox Material Light",
    label: "Gruvbox Material Light",
    labelKey: "settings.terminal_theme.preset.gruvbox_material_light",
  },
  { value: "Nord Light", label: "Nord Light", labelKey: "settings.terminal_theme.preset.nord_light" },
  { value: "Atom One Light", label: "Atom One Light", labelKey: "settings.terminal_theme.preset.atom_one_light" },
];

export const backgroundColorOptions: BackgroundColorOption[] = [
  { label: "Auto", dark: "", light: "", preview: "linear-gradient(135deg, #18181b, #f4f4f5)" },
  { label: "Zinc", dark: "#18181b", light: "#f4f4f5", preview: "linear-gradient(135deg, #18181b, #f4f4f5)" },
  { label: "Neutral", dark: "#171717", light: "#f5f5f5", preview: "linear-gradient(135deg, #171717, #f5f5f5)" },
  { label: "Stone", dark: "#1c1917", light: "#f5f5f4", preview: "linear-gradient(135deg, #1c1917, #f5f5f4)" },
  { label: "Slate", dark: "#0f172a", light: "#f1f5f9", preview: "linear-gradient(135deg, #0f172a, #f1f5f9)" },
];

export const themeColorOptions: ThemeColorOption[] = [
  { label: "Blue", color: "#3b82f6" },
  { label: "Sky", color: "#0ea5e9" },
  { label: "Cyan", color: "#06b6d4" },
  { label: "Teal", color: "#14b8a6" },
  { label: "Emerald", color: "#10b981" },
  { label: "Green", color: "#22c55e" },
  { label: "Lime", color: "#84cc16" },
  { label: "Amber", color: "#f59e0b" },
  { label: "Orange", color: "#f97316" },
  { label: "Red", color: "#ef4444" },
  { label: "Rose", color: "#f43f5e" },
  { label: "Pink", color: "#ec4899" },
  { label: "Fuchsia", color: "#d946ef" },
  { label: "Purple", color: "#a855f7" },
  { label: "Violet", color: "#8b5cf6" },
  { label: "Indigo", color: "#6366f1" },
];

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

export const systemThemeOptions = [
  { value: "Auto", label: "System", labelKey: "settings.theme.system" },
] satisfies TerminalThemeOption[];

const managedThemeVariables: ManagedThemeVariable[] = [
  "--terminal-bg",
  "--terminal-fg",
  "--terminal-cursor",
  "--terminal-selection",
  "--terminal-black",
  "--terminal-red",
  "--terminal-green",
  "--terminal-yellow",
  "--terminal-blue",
  "--terminal-magenta",
  "--terminal-cyan",
  "--terminal-white",
  "--terminal-bright-black",
  "--terminal-bright-red",
  "--terminal-bright-green",
  "--terminal-bright-yellow",
  "--terminal-bright-blue",
  "--terminal-bright-magenta",
  "--terminal-bright-cyan",
  "--terminal-bright-white",
  "--editor-comment",
  "--editor-keyword",
  "--editor-atom",
  "--editor-string",
  "--editor-string2",
  "--editor-number",
  "--editor-variable",
  "--editor-variable2",
  "--editor-type",
  "--editor-class",
  "--editor-property",
  "--editor-operator",
  "--editor-punctuation",
  "--editor-meta",
  "--editor-link",
  "--editor-heading",
  "--editor-inserted",
  "--editor-deleted",
  "--editor-invalid",
  "--surface-window-tint",
  "--surface-window-glass",
  "--surface-editor",
  "--color-brand-blue",
  "--color-brand-blue-deep",
  "--color-on-brand",
  "--accent",
  "--accent-hover",
  "--accent-foreground",
  "--accent-soft",
  "--accent-soft-foreground",
  "--focus",
  "--link",
  "--color-surface-glass",
  "--color-surface-chrome",
  "--color-surface-panel",
  "--color-surface-card",
  "--color-surface-terminal",
  "--color-surface-editor",
];

const terminalThemeProfiles: Record<string, TerminalThemeProfile | string> = {
  auto: { appTheme: "system", variables: {} },
  "tokyonight storm": terminalProfile("midnight", {
    background: "#24283b",
    foreground: "#c0caf5",
    cursor: "#c0caf5",
    selection: "#364a82",
    black: "#1d202f",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#c0caf5",
    brightBlack: "#565f89",
    brightRed: "#ff7a93",
    brightGreen: "#b9f27c",
    brightYellow: "#ffcb6b",
    brightBlue: "#7da6ff",
    brightMagenta: "#c8a7ff",
    brightCyan: "#90e0ff",
    brightWhite: "#dfe5ff",
  }),
  "tokyo night storm": "tokyonight storm",
  "tokyonight night": terminalProfile("midnight", {
    background: "#1a1b26",
    foreground: "#c0caf5",
    cursor: "#c0caf5",
    selection: "#33467c",
    black: "#15161e",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#c0caf5",
    brightBlack: "#565f89",
    brightRed: "#ff7a93",
    brightGreen: "#b9f27c",
    brightYellow: "#ffcb6b",
    brightBlue: "#7da6ff",
    brightMagenta: "#c8a7ff",
    brightCyan: "#90e0ff",
    brightWhite: "#dfe5ff",
  }),
  "tokyo night night": "tokyonight night",
  "catppuccin mocha": terminalProfile("midnight", {
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    cursor: "#f5e0dc",
    selection: "#45475a",
    black: "#181825",
    red: "#f38ba8",
    green: "#a6e3a1",
    yellow: "#f9e2af",
    blue: "#89b4fa",
    magenta: "#cba6f7",
    cyan: "#94e2d5",
    white: "#bac2de",
    brightBlack: "#585b70",
    brightRed: "#f38ba8",
    brightGreen: "#a6e3a1",
    brightYellow: "#f9e2af",
    brightBlue: "#89b4fa",
    brightMagenta: "#cba6f7",
    brightCyan: "#94e2d5",
    brightWhite: "#a6adc8",
  }),
  "catppuccin latte": terminalProfile("light", {
    background: "#eff1f5",
    foreground: "#4c4f69",
    cursor: "#dc8a78",
    selection: "#ccd0da",
    black: "#5c5f77",
    red: "#d20f39",
    green: "#40a02b",
    yellow: "#df8e1d",
    blue: "#1e66f5",
    magenta: "#8839ef",
    cyan: "#179299",
    white: "#acb0be",
    brightBlack: "#6c6f85",
    brightRed: "#d20f39",
    brightGreen: "#40a02b",
    brightYellow: "#df8e1d",
    brightBlue: "#1e66f5",
    brightMagenta: "#8839ef",
    brightCyan: "#179299",
    brightWhite: "#bcc0cc",
  }),
  "rose pine moon": terminalProfile("midnight", {
    background: "#232136",
    foreground: "#e0def4",
    cursor: "#c4a7e7",
    selection: "#393552",
    black: "#393552",
    red: "#eb6f92",
    green: "#9ccfd8",
    yellow: "#f6c177",
    blue: "#3e8fb0",
    magenta: "#c4a7e7",
    cyan: "#ea9a97",
    white: "#e0def4",
    brightBlack: "#6e6a86",
    brightRed: "#eb6f92",
    brightGreen: "#9ccfd8",
    brightYellow: "#f6c177",
    brightBlue: "#3e8fb0",
    brightMagenta: "#c4a7e7",
    brightCyan: "#ea9a97",
    brightWhite: "#e0def4",
  }),
  "kanagawa wave": terminalProfile("midnight", {
    background: "#1f1f28",
    foreground: "#dcd7ba",
    cursor: "#c8c093",
    selection: "#2d4f67",
    black: "#090618",
    red: "#c34043",
    green: "#76946a",
    yellow: "#c0a36e",
    blue: "#7e9cd8",
    magenta: "#957fb8",
    cyan: "#6a9589",
    white: "#c8c093",
    brightBlack: "#727169",
    brightRed: "#e82424",
    brightGreen: "#98bb6c",
    brightYellow: "#e6c384",
    brightBlue: "#7fb4ca",
    brightMagenta: "#938aa9",
    brightCyan: "#7aa89f",
    brightWhite: "#dcd7ba",
  }),
  "material ocean": terminalProfile("midnight", {
    background: "#0f111a",
    foreground: "#8f93a2",
    cursor: "#ffcc00",
    selection: "#1f2233",
    black: "#000000",
    red: "#ff5370",
    green: "#c3e88d",
    yellow: "#ffcb6b",
    blue: "#82aaff",
    magenta: "#c792ea",
    cyan: "#89ddff",
    white: "#ffffff",
    brightBlack: "#546e7a",
    brightRed: "#ff869a",
    brightGreen: "#ddffa7",
    brightYellow: "#ffd98f",
    brightBlue: "#9cc4ff",
    brightMagenta: "#d6a8ff",
    brightCyan: "#a6eaff",
    brightWhite: "#ffffff",
  }),
  "ayu mirage": terminalProfile("midnight", {
    background: "#1f2430",
    foreground: "#cbccc6",
    cursor: "#ffcc66",
    selection: "#33415e",
    black: "#191e2a",
    red: "#f28779",
    green: "#bae67e",
    yellow: "#ffd580",
    blue: "#73d0ff",
    magenta: "#d4bfff",
    cyan: "#95e6cb",
    white: "#c7c7c7",
    brightBlack: "#686868",
    brightRed: "#f28779",
    brightGreen: "#bae67e",
    brightYellow: "#ffd580",
    brightBlue: "#73d0ff",
    brightMagenta: "#d4bfff",
    brightCyan: "#95e6cb",
    brightWhite: "#ffffff",
  }),
  dracula: terminalProfile("midnight", {
    background: "#282a36",
    foreground: "#f8f8f2",
    cursor: "#f8f8f2",
    selection: "#44475a",
    black: "#21222c",
    red: "#ff5555",
    green: "#50fa7b",
    yellow: "#f1fa8c",
    blue: "#bd93f9",
    magenta: "#ff79c6",
    cyan: "#8be9fd",
    white: "#f8f8f2",
    brightBlack: "#6272a4",
    brightRed: "#ff6e6e",
    brightGreen: "#69ff94",
    brightYellow: "#ffffa5",
    brightBlue: "#d6acff",
    brightMagenta: "#ff92df",
    brightCyan: "#a4ffff",
    brightWhite: "#ffffff",
  }),
  "dracula+": "dracula",
  "github dark": terminalProfile("dark", {
    background: "#0d1117",
    foreground: "#c9d1d9",
    cursor: "#c9d1d9",
    selection: "#264f78",
    black: "#484f58",
    red: "#ff7b72",
    green: "#3fb950",
    yellow: "#d29922",
    blue: "#58a6ff",
    magenta: "#bc8cff",
    cyan: "#39c5cf",
    white: "#b1bac4",
    brightBlack: "#6e7681",
    brightRed: "#ffa198",
    brightGreen: "#56d364",
    brightYellow: "#e3b341",
    brightBlue: "#79c0ff",
    brightMagenta: "#d2a8ff",
    brightCyan: "#56d4dd",
    brightWhite: "#f0f6fc",
  }),
  "gruvbox dark": terminalProfile("graphite", {
    background: "#282828",
    foreground: "#ebdbb2",
    cursor: "#fabd2f",
    selection: "#504945",
    black: "#282828",
    red: "#cc241d",
    green: "#98971a",
    yellow: "#d79921",
    blue: "#458588",
    magenta: "#b16286",
    cyan: "#689d6a",
    white: "#a89984",
    brightBlack: "#928374",
    brightRed: "#fb4934",
    brightGreen: "#b8bb26",
    brightYellow: "#fabd2f",
    brightBlue: "#83a598",
    brightMagenta: "#d3869b",
    brightCyan: "#8ec07c",
    brightWhite: "#ebdbb2",
  }),
  "gruvbox material dark": terminalProfile("graphite", {
    background: "#1d2021",
    foreground: "#d4be98",
    cursor: "#d4be98",
    selection: "#3c3836",
    black: "#32302f",
    red: "#ea6962",
    green: "#a9b665",
    yellow: "#d8a657",
    blue: "#7daea3",
    magenta: "#d3869b",
    cyan: "#89b482",
    white: "#d4be98",
    brightBlack: "#665c54",
    brightRed: "#ea6962",
    brightGreen: "#a9b665",
    brightYellow: "#d8a657",
    brightBlue: "#7daea3",
    brightMagenta: "#d3869b",
    brightCyan: "#89b482",
    brightWhite: "#ddc7a1",
  }),
  nord: terminalProfile("graphite", {
    background: "#2e3440",
    foreground: "#d8dee9",
    cursor: "#d8dee9",
    selection: "#4c566a",
    black: "#3b4252",
    red: "#bf616a",
    green: "#a3be8c",
    yellow: "#ebcb8b",
    blue: "#81a1c1",
    magenta: "#b48ead",
    cyan: "#88c0d0",
    white: "#e5e9f0",
    brightBlack: "#4c566a",
    brightRed: "#bf616a",
    brightGreen: "#a3be8c",
    brightYellow: "#ebcb8b",
    brightBlue: "#81a1c1",
    brightMagenta: "#b48ead",
    brightCyan: "#8fbcbb",
    brightWhite: "#eceff4",
  }),
  "tokyonight day": terminalProfile("light", {
    background: "#e1e2e7",
    foreground: "#3760bf",
    cursor: "#3760bf",
    selection: "#b7c1e3",
    black: "#e9e9ed",
    red: "#f52a65",
    green: "#587539",
    yellow: "#8c6c3e",
    blue: "#2e7de9",
    magenta: "#9854f1",
    cyan: "#007197",
    white: "#6172b0",
    brightBlack: "#a1a6c5",
    brightRed: "#f52a65",
    brightGreen: "#587539",
    brightYellow: "#8c6c3e",
    brightBlue: "#2e7de9",
    brightMagenta: "#9854f1",
    brightCyan: "#007197",
    brightWhite: "#3760bf",
  }),
  "tokyo night day": "tokyonight day",
  "github light": terminalProfile("light", {
    background: "#ffffff",
    foreground: "#24292f",
    cursor: "#0969da",
    selection: "#b6d7ff",
    black: "#24292f",
    red: "#cf222e",
    green: "#116329",
    yellow: "#4d2d00",
    blue: "#0969da",
    magenta: "#8250df",
    cyan: "#1b7c83",
    white: "#6e7781",
    brightBlack: "#57606a",
    brightRed: "#a40e26",
    brightGreen: "#1a7f37",
    brightYellow: "#9a6700",
    brightBlue: "#218bff",
    brightMagenta: "#a475f9",
    brightCyan: "#3192aa",
    brightWhite: "#f6f8fa",
  }),
  "flexoki dark": terminalProfile("dark", {
    background: "#100f0f",
    foreground: "#cecdc3",
    cursor: "#cecdc3",
    selection: "#403e3c",
    black: "#100f0f",
    red: "#af3029",
    green: "#66800b",
    yellow: "#ad8301",
    blue: "#205ea6",
    magenta: "#5e409d",
    cyan: "#24837b",
    white: "#cecdc3",
    brightBlack: "#575653",
    brightRed: "#d14d41",
    brightGreen: "#879a39",
    brightYellow: "#d0a215",
    brightBlue: "#4385be",
    brightMagenta: "#8b7ec8",
    brightCyan: "#3aa99f",
    brightWhite: "#fffcf0",
  }),
  "flexoki light": terminalProfile("light", {
    background: "#fffcf0",
    foreground: "#100f0f",
    cursor: "#100f0f",
    selection: "#e6e4d9",
    black: "#100f0f",
    red: "#af3029",
    green: "#66800b",
    yellow: "#ad8301",
    blue: "#205ea6",
    magenta: "#5e409d",
    cyan: "#24837b",
    white: "#cecdc3",
    brightBlack: "#6f6e69",
    brightRed: "#d14d41",
    brightGreen: "#879a39",
    brightYellow: "#d0a215",
    brightBlue: "#4385be",
    brightMagenta: "#8b7ec8",
    brightCyan: "#3aa99f",
    brightWhite: "#fffcf0",
  }),
  "gruvbox light": terminalProfile("light", {
    background: "#fbf1c7",
    foreground: "#3c3836",
    cursor: "#3c3836",
    selection: "#d5c4a1",
    black: "#fbf1c7",
    red: "#cc241d",
    green: "#98971a",
    yellow: "#d79921",
    blue: "#458588",
    magenta: "#b16286",
    cyan: "#689d6a",
    white: "#7c6f64",
    brightBlack: "#928374",
    brightRed: "#9d0006",
    brightGreen: "#79740e",
    brightYellow: "#b57614",
    brightBlue: "#076678",
    brightMagenta: "#8f3f71",
    brightCyan: "#427b58",
    brightWhite: "#3c3836",
  }),
  "gruvbox material light": terminalProfile("light", {
    background: "#fbf1c7",
    foreground: "#654735",
    cursor: "#654735",
    selection: "#d5c4a1",
    black: "#fbf1c7",
    red: "#c14a4a",
    green: "#6c782e",
    yellow: "#b47109",
    blue: "#45707a",
    magenta: "#945e80",
    cyan: "#4c7a5d",
    white: "#654735",
    brightBlack: "#928374",
    brightRed: "#c14a4a",
    brightGreen: "#6c782e",
    brightYellow: "#b47109",
    brightBlue: "#45707a",
    brightMagenta: "#945e80",
    brightCyan: "#4c7a5d",
    brightWhite: "#3c3836",
  }),
  "nord light": terminalProfile("light", {
    background: "#eceff4",
    foreground: "#2e3440",
    cursor: "#2e3440",
    selection: "#d8dee9",
    black: "#3b4252",
    red: "#bf616a",
    green: "#a3be8c",
    yellow: "#d08770",
    blue: "#5e81ac",
    magenta: "#b48ead",
    cyan: "#8fbcbb",
    white: "#e5e9f0",
    brightBlack: "#4c566a",
    brightRed: "#bf616a",
    brightGreen: "#a3be8c",
    brightYellow: "#ebcb8b",
    brightBlue: "#81a1c1",
    brightMagenta: "#b48ead",
    brightCyan: "#88c0d0",
    brightWhite: "#eceff4",
  }),
  "atom one light": terminalProfile("light", {
    background: "#fafafa",
    foreground: "#383a42",
    cursor: "#526fff",
    selection: "#e5e5e6",
    black: "#383a42",
    red: "#e45649",
    green: "#50a14f",
    yellow: "#c18401",
    blue: "#4078f2",
    magenta: "#a626a4",
    cyan: "#0184bc",
    white: "#a0a1a7",
    brightBlack: "#696c77",
    brightRed: "#e45649",
    brightGreen: "#50a14f",
    brightYellow: "#c18401",
    brightBlue: "#4078f2",
    brightMagenta: "#a626a4",
    brightCyan: "#0184bc",
    brightWhite: "#f0f0f0",
  }),
};

export function applyTheme(theme: AppTheme) {
  const root = document.documentElement;
  root.classList.toggle("dark", theme !== "light");
  root.dataset.theme = theme;
}

export function resolveSystemTheme(): AppTheme {
  if (typeof window === "undefined") {
    return "dark";
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function resolveConfiguredTheme(theme: string, systemTheme = resolveSystemTheme()): AppTheme {
  const profile = resolveTerminalThemeProfile(theme);
  return profile.appTheme === "system" ? systemTheme : profile.appTheme;
}

export function terminalThemePreview(theme: string): TerminalThemePreview {
  const profile = resolveTerminalThemeProfile(theme);
  const variables = profile.variables;
  const systemFallback = resolveSystemTheme() === "light" ? "#fffcf0" : "#100f0f";
  const background = variables["--terminal-bg"] ?? systemFallback;
  const foreground = variables["--terminal-fg"] ?? (profile.appTheme === "light" ? "#100f0f" : "#cecdc3");
  return {
    background,
    foreground,
    mutedForeground: mixColor(foreground, background, profile.appTheme === "light" ? 0.42 : 0.36),
    selection: variables["--terminal-selection"] ?? mixColor(foreground, background, 0.78),
    appTheme: profile.appTheme,
  };
}

export function applyConfiguredTheme(settings: AppSettings, systemTheme = resolveSystemTheme()) {
  const root = document.documentElement;
  for (const variable of managedThemeVariables) {
    root.style.removeProperty(variable);
  }

  const appTheme = resolveConfiguredTheme(settings.theme, systemTheme);
  applyTheme(appTheme);

  const profile = resolveTerminalThemeProfile(settings.theme);
  for (const [variable, value] of Object.entries(profile.variables)) {
    root.style.setProperty(variable, value);
  }

  applyAppearanceOverrides(root, settings.background, settings.themeColor);
  applyNativeWindowTheme(settings, appTheme);
}

export function initSystemTheme(settingsProvider: () => AppSettings = readAppSettings) {
  if (typeof window === "undefined") {
    return () => undefined;
  }

  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const sync = () => applyConfiguredTheme(settingsProvider(), media.matches ? "dark" : "light");
  sync();

  if (typeof media.addEventListener === "function") {
    media.addEventListener("change", sync);
    return () => {
      media.removeEventListener("change", sync);
    };
  }

  media.addListener(sync);
  return () => {
    media.removeListener(sync);
  };
}

function terminalProfile(
  appTheme: AppTheme,
  palette: {
    background: string;
    foreground: string;
    cursor: string;
    selection: string;
    black: string;
    red: string;
    green: string;
    yellow: string;
    blue: string;
    magenta: string;
    cyan: string;
    white: string;
    brightBlack: string;
    brightRed: string;
    brightGreen: string;
    brightYellow: string;
    brightBlue: string;
    brightMagenta: string;
    brightCyan: string;
    brightWhite: string;
  },
): TerminalThemeProfile {
  return {
    appTheme,
    variables: {
      "--terminal-bg": palette.background,
      "--terminal-fg": palette.foreground,
      "--terminal-cursor": palette.cursor,
      "--terminal-selection": palette.selection,
      "--terminal-black": palette.black,
      "--terminal-red": palette.red,
      "--terminal-green": palette.green,
      "--terminal-yellow": palette.yellow,
      "--terminal-blue": palette.blue,
      "--terminal-magenta": palette.magenta,
      "--terminal-cyan": palette.cyan,
      "--terminal-white": palette.white,
      "--terminal-bright-black": palette.brightBlack,
      "--terminal-bright-red": palette.brightRed,
      "--terminal-bright-green": palette.brightGreen,
      "--terminal-bright-yellow": palette.brightYellow,
      "--terminal-bright-blue": palette.brightBlue,
      "--terminal-bright-magenta": palette.brightMagenta,
      "--terminal-bright-cyan": palette.brightCyan,
      "--terminal-bright-white": palette.brightWhite,
      "--editor-comment": palette.brightBlack,
      "--editor-keyword": palette.magenta,
      "--editor-atom": palette.brightMagenta,
      "--editor-string": palette.green,
      "--editor-string2": palette.cyan,
      "--editor-number": palette.yellow,
      "--editor-variable": palette.foreground,
      "--editor-variable2": palette.brightCyan,
      "--editor-type": palette.blue,
      "--editor-class": palette.brightYellow,
      "--editor-property": palette.cyan,
      "--editor-operator": palette.red,
      "--editor-punctuation": mixColor(palette.foreground, palette.background, 0.28),
      "--editor-meta": palette.brightBlack,
      "--editor-link": palette.blue,
      "--editor-heading": palette.brightBlue,
      "--editor-inserted": palette.green,
      "--editor-deleted": palette.red,
      "--editor-invalid": palette.brightRed,
      "--color-surface-terminal": palette.background,
      "--surface-editor": palette.background,
      "--color-surface-editor": palette.background,
    },
  };
}

function resolveTerminalThemeProfile(theme: string): TerminalThemeProfile {
  const normalized = normalizeThemeName(theme);
  const profile = terminalThemeProfiles[normalized];
  if (typeof profile === "string") {
    return terminalThemeProfiles[profile] as TerminalThemeProfile;
  }
  if (profile) return profile;
  if (
    normalized.includes("day") ||
    normalized.includes("latte") ||
    normalized.includes("dawn") ||
    normalized.includes("light")
  ) {
    return terminalThemeProfiles["catppuccin latte"] as TerminalThemeProfile;
  }
  if (normalized.includes("nord")) {
    return terminalThemeProfiles.nord as TerminalThemeProfile;
  }
  if (
    normalized.includes("night") ||
    normalized.includes("mocha") ||
    normalized.includes("moon") ||
    normalized.includes("wave") ||
    normalized.includes("dark")
  ) {
    return terminalThemeProfiles["tokyonight night"] as TerminalThemeProfile;
  }
  return terminalThemeProfiles.auto as TerminalThemeProfile;
}

function applyAppearanceOverrides(root: HTMLElement, background: string, themeColor: string) {
  applyBackgroundOverride(root, background);
  applyThemeColorOverride(root, themeColor);
}

function applyBackgroundOverride(root: HTMLElement, background: string) {
  const option = resolveBackgroundColorOption(background);
  const appTheme = root.dataset.theme === "light" ? "light" : "dark";
  if (!option || normalizeThemeName(option.label) === "auto") {
    applyDerivedSurfacePalette(root, appTheme);
    root.style.setProperty(
      "--surface-window-tint",
      appTheme === "light"
        ? "color-mix(in oklab, var(--terminal-bg) 94%, black)"
        : "color-mix(in oklab, var(--terminal-bg) 88%, white)",
    );
    root.style.setProperty(
      "--surface-window-glass",
      appTheme === "light"
        ? "color-mix(in oklab, var(--terminal-bg) 97%, black)"
        : "color-mix(in oklab, var(--terminal-bg) 94%, white)",
    );
    return;
  }
  const color = appTheme === "light" ? option.light : option.dark;
  const anchor = appTheme === "light" ? "rgb(255 255 255)" : "var(--terminal-bg)";
  root.style.setProperty(
    "--surface-window-tint",
    `color-mix(in oklab, ${color} ${appTheme === "light" ? "18%" : "22%"}, ${anchor})`,
  );
  root.style.setProperty(
    "--surface-window-glass",
    `color-mix(in oklab, ${color} ${appTheme === "light" ? "12%" : "16%"}, ${anchor})`,
  );
  root.style.setProperty(
    "--color-surface-glass",
    `color-mix(in oklab, ${color} ${appTheme === "light" ? "36%" : "58%"}, ${anchor})`,
  );
  root.style.setProperty(
    "--color-surface-chrome",
    `color-mix(in oklab, ${color} ${appTheme === "light" ? "30%" : "46%"}, ${anchor})`,
  );
  root.style.setProperty(
    "--color-surface-panel",
    `color-mix(in oklab, ${color} ${appTheme === "light" ? "22%" : "32%"}, ${anchor})`,
  );
  root.style.setProperty(
    "--color-surface-card",
    `color-mix(in oklab, ${color} ${appTheme === "light" ? "12%" : "22%"}, ${anchor})`,
  );
  root.style.setProperty("--color-surface-terminal", "var(--terminal-bg)");
  root.style.setProperty("--color-surface-editor", "var(--terminal-bg)");
  root.style.setProperty("--surface-editor", "var(--terminal-bg)");
  applyDerivedBorderPalette(root, appTheme);
}

function applyDerivedSurfacePalette(root: HTMLElement, appTheme: "light" | "dark") {
  const chromeMix = appTheme === "light" ? "95%, black" : "86%, white";
  const panelMix = appTheme === "light" ? "97%, black" : "90%, white";
  const cardMix = appTheme === "light" ? "98%, black" : "94%, white";
  root.style.setProperty("--color-surface-glass", `color-mix(in oklab, var(--terminal-bg) ${chromeMix})`);
  root.style.setProperty("--color-surface-chrome", `color-mix(in oklab, var(--terminal-bg) ${chromeMix})`);
  root.style.setProperty("--color-surface-panel", `color-mix(in oklab, var(--terminal-bg) ${panelMix})`);
  root.style.setProperty("--color-surface-card", `color-mix(in oklab, var(--terminal-bg) ${cardMix})`);
  root.style.setProperty("--color-surface-terminal", "var(--terminal-bg)");
  root.style.setProperty("--color-surface-editor", "var(--terminal-bg)");
  applyDerivedBorderPalette(root, appTheme);
}

function applyDerivedBorderPalette(root: HTMLElement, appTheme: "light" | "dark") {
  root.style.setProperty(
    "--color-line",
    appTheme === "light"
      ? "color-mix(in oklab, var(--terminal-bg) 86%, black)"
      : "color-mix(in oklab, var(--terminal-bg) 76%, white)",
  );
  root.style.setProperty(
    "--color-line-strong",
    appTheme === "light"
      ? "color-mix(in oklab, var(--terminal-bg) 76%, black)"
      : "color-mix(in oklab, var(--terminal-bg) 64%, white)",
  );
}

function applyThemeColorOverride(root: HTMLElement, themeColor: string) {
  const option = resolveThemeColorOption(themeColor);
  const color = option?.color ?? "#3b82f6";
  const foreground = resolveAccentForeground(color);
  root.style.setProperty("--color-brand-blue", color);
  root.style.setProperty("--color-brand-blue-deep", `color-mix(in oklab, ${color} 78%, black)`);
  root.style.setProperty("--color-on-brand", foreground);
  applyAccentColor(root, color, foreground);
}

function applyAccentColor(root: HTMLElement, color: string, foreground: string) {
  root.style.setProperty("--accent", color);
  root.style.setProperty("--accent-hover", `color-mix(in oklab, ${color} 88%, ${foreground})`);
  root.style.setProperty("--accent-foreground", foreground);
  root.style.setProperty("--accent-soft", `color-mix(in oklab, ${color} 16%, transparent)`);
  root.style.setProperty("--accent-soft-foreground", color);
  root.style.setProperty("--focus", color);
  root.style.setProperty("--link", color);
}

function resolveAccentForeground(color: string) {
  const parsed = parseHexColor(color);
  if (!parsed) return "rgb(252 255 255)";
  const luminance = relativeLuminance(parsed.r, parsed.g, parsed.b);
  return luminance > 0.46 ? "rgb(28 35 46)" : "rgb(252 255 255)";
}

export function resolveBackgroundColorOption(background: string): BackgroundColorOption | undefined {
  const normalized = normalizeThemeName(background);
  return backgroundColorOptions.find((item) => normalizeThemeName(item.label) === normalized);
}

export function resolveThemeColorOption(themeColor: string): ThemeColorOption | undefined {
  const normalized = normalizeThemeName(themeColor);
  const directOption = themeColorOptions.find((item) => normalizeThemeName(item.label) === normalized);
  if (directOption) return directOption;

  const alias = legacyThemeColorAliases[normalized];
  if (!alias) return undefined;
  return themeColorOptions.find((item) => normalizeThemeName(item.label) === normalizeThemeName(alias));
}

function applyNativeWindowTheme(settings: AppSettings, appTheme: AppTheme) {
  if (!window.__TAURI_INTERNALS__) return;

  const route = window.location.hash.replace(/^#/, "");
  if (route) return;

  const currentWindow = getCurrentWindow();
  const nativeTheme: Theme | null =
    resolveTerminalThemeProfile(settings.theme).appTheme === "system" ? null : appTheme === "light" ? "light" : "dark";
  void currentWindow.setTheme(nativeTheme).catch((error) => {
    console.error("failed to apply native window theme", error);
  });
  const background =
    getComputedStyle(document.documentElement).getPropertyValue("--surface-window-tint").trim() ||
    (appTheme === "light" ? "#fbfcfe" : "#171b22");
  void currentWindow.setBackgroundColor(background).catch((error) => {
    console.error("failed to apply native window background", error);
  });
}

function normalizeThemeName(value: string) {
  return value.trim().toLowerCase().replace(/[_-]+/g, " ").replace(/\s+/g, " ");
}

function mixColor(foreground: string, background: string, backgroundRatio: number) {
  const fg = parseHexColor(foreground);
  const bg = parseHexColor(background);
  if (!fg || !bg) return foreground;
  const foregroundRatio = 1 - backgroundRatio;
  return `rgb(${Math.round(fg.r * foregroundRatio + bg.r * backgroundRatio)} ${Math.round(
    fg.g * foregroundRatio + bg.g * backgroundRatio,
  )} ${Math.round(fg.b * foregroundRatio + bg.b * backgroundRatio)})`;
}

function parseHexColor(value: string) {
  const normalized = value.trim().replace(/^#/, "");
  if (!/^[0-9a-f]{6}$/i.test(normalized)) return null;
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16),
  };
}

function relativeLuminance(r: number, g: number, b: number) {
  const [red, green, blue] = [r, g, b].map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}
