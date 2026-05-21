import { invoke } from "@tauri-apps/api/core";
import { open, save, type OpenDialogOptions, type SaveDialogOptions } from "@tauri-apps/plugin-dialog";
import { tm } from "./i18n";

type LocalizedDialogLabels = {
  prompt?: string;
  message?: string;
};

type NativeOpenDialogOptions = OpenDialogOptions & LocalizedDialogLabels;
type NativeSaveDialogOptions = SaveDialogOptions & LocalizedDialogLabels;

export async function openLocalizedDialog<T extends OpenDialogOptions>(options: T & LocalizedDialogLabels) {
  if (window.__TAURI_INTERNALS__ && isMacPlatform()) {
    const result = await invoke<string[] | null>("localized_open_dialog", {
      request: localizedOpenRequest(options),
    });
    if (!result) return null;
    return (options.multiple ? result : (result[0] ?? null)) as Awaited<ReturnType<typeof open<T>>>;
  }
  return open(tauriOpenOptions(options));
}

export async function saveLocalizedDialog(options: NativeSaveDialogOptions) {
  if (window.__TAURI_INTERNALS__ && isMacPlatform()) {
    return invoke<string | null>("localized_save_dialog", {
      request: localizedSaveRequest(options),
    });
  }
  return save(tauriSaveOptions(options));
}

function tauriOpenOptions(options: NativeOpenDialogOptions): OpenDialogOptions {
  const { prompt: _prompt, message: _message, ...nativeOptions } = options;
  return nativeOptions;
}

function tauriSaveOptions(options: NativeSaveDialogOptions): SaveDialogOptions {
  const { prompt: _prompt, message: _message, ...nativeOptions } = options;
  return nativeOptions;
}

function localizedOpenRequest(options: NativeOpenDialogOptions) {
  return {
    title: options.title ?? "",
    message: options.message ?? options.title ?? "",
    prompt: options.prompt ?? (options.directory ? tm("common.choose", "Choose") : tm("common.open", "Open")),
    defaultPath: options.defaultPath ?? null,
    filters: options.filters ?? [],
    directory: Boolean(options.directory),
    multiple: Boolean(options.multiple),
    canCreateDirectories: options.canCreateDirectories ?? null,
  };
}

function localizedSaveRequest(options: NativeSaveDialogOptions) {
  return {
    title: options.title ?? "",
    message: options.message ?? options.title ?? "",
    prompt: options.prompt ?? tm("common.save", "Save"),
    defaultPath: options.defaultPath ?? null,
    filters: options.filters ?? [],
    canCreateDirectories: options.canCreateDirectories ?? null,
  };
}

function isMacPlatform() {
  return navigator.platform.toLowerCase().includes("mac");
}
