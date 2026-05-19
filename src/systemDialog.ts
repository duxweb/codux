import { confirm, message, type ConfirmDialogOptions, type MessageDialogOptions } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { tm } from "./i18n";

type NativeConfirmOptions = ConfirmDialogOptions & {
  fallbackTitle?: string;
};

type NativeMessageOptions = MessageDialogOptions & {
  fallbackTitle?: string;
};

export async function systemConfirm(text: string, options?: NativeConfirmOptions) {
  if (!window.__TAURI_INTERNALS__) {
    return window.confirm(text);
  }
  return confirm(text, {
    title: options?.title ?? options?.fallbackTitle ?? "Codux",
    kind: options?.kind,
    okLabel: options?.okLabel,
    cancelLabel: options?.cancelLabel,
  });
}

export async function systemMessage(text: string, options?: NativeMessageOptions) {
  if (!window.__TAURI_INTERNALS__) {
    window.alert(text);
    return "Ok";
  }
  return message(text, {
    title: options?.title ?? options?.fallbackTitle ?? "Codux",
    kind: options?.kind,
    buttons: options?.buttons,
    okLabel: options?.okLabel,
  });
}

export async function restartNotice(
  text = tm("settings.theme.restart_required", "Restart the app to apply this setting."),
  title = tm("settings.theme.restart_title", "Restart Required"),
) {
  const shouldRestart = await systemConfirm(text, {
    title,
    kind: "info",
    okLabel: tm("common.restart_now", "Restart Now"),
    cancelLabel: tm("common.later", "Later"),
  });
  if (!shouldRestart) return;
  if (!window.__TAURI_INTERNALS__) {
    window.location.reload();
    return;
  }
  await invoke("app_request_restart");
}
