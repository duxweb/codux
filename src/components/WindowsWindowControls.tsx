import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MinusSmall, Square2Stack, X, type AppIcon } from "../icons";
import { tm } from "../i18n";
import { PressableButton } from "./PressableButton";

type Props = {
  className?: string;
};

export function WindowsWindowControls({ className = "" }: Props) {
  const minimize = () => {
    if (!window.__TAURI_INTERNALS__) return;
    void getCurrentWindow().minimize();
  };
  const toggleMaximize = () => {
    if (!window.__TAURI_INTERNALS__) return;
    void getCurrentWindow().toggleMaximize();
  };
  const close = () => {
    if (!window.__TAURI_INTERNALS__) return;
    void invoke("app_window_close").catch((error) => console.error("failed to close window", error));
  };

  return (
    <div className={`absolute right-0 top-0 z-50 flex items-start no-drag ${className}`}>
      <WindowControlButton icon={MinusSmall} label={tm("window.minimize", "Minimize")} onPress={minimize} />
      <WindowControlButton icon={Square2Stack} label={tm("window.maximize", "Maximize")} onPress={toggleMaximize} />
      <WindowControlButton icon={X} label={tm("common.close", "Close")} danger onPress={close} />
    </div>
  );
}

function WindowControlButton({
  icon: Icon,
  label,
  danger,
  onPress,
}: {
  icon: AppIcon;
  label: string;
  danger?: boolean;
  onPress: () => void;
}) {
  return (
    <PressableButton
      type="button"
      aria-label={label}
      title={label}
      className={`grid h-[34px] w-[46px] place-items-center text-ink-soft transition-colors ${
        danger ? "hover:bg-brand-red hover:text-white" : "hover:bg-fill/10 hover:text-ink"
      }`}
      onPointerDownCapture={(event) => {
        event.stopPropagation();
      }}
      onPress={onPress}
    >
      <Icon size={13} strokeWidth={2} />
    </PressableButton>
  );
}
