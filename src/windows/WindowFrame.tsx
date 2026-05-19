import type { ReactNode } from "react";
import { Button } from "../components/Button";
import { tm } from "../i18n";

type WindowFrameProps = {
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  mainClassName?: string;
  mainScrollable?: boolean;
};

export function WindowFrame({
  title,
  children,
  footer,
  mainClassName,
  mainScrollable = true,
}: WindowFrameProps) {
  const titleInsetClass = isMacPlatform() ? "pl-[86px] pr-5" : "px-5";

  return (
    <div className="h-screen overflow-hidden bg-surface-chrome/92 text-ink flex flex-col">
      <header
        className="h-10 flex flex-shrink-0 items-start border-b border-line/45 bg-surface-chrome/92 pt-[12px] drag-region"
        data-tauri-drag-region
      >
        <div className={`min-w-0 w-full drag-region ${titleInsetClass}`} data-tauri-drag-region>
          <div className="truncate text-[13.5px] font-semibold leading-none tracking-tight drag-region" data-tauri-drag-region>{title}</div>
        </div>
      </header>

      <main
        className={`min-h-0 flex-1 flex flex-col ${mainScrollable ? "overflow-y-auto" : "overflow-hidden"} no-drag ${mainClassName ?? "px-6 py-5"}`}
      >
        {children}
      </main>

      {footer !== undefined && (
        <footer className="min-h-[62px] flex-shrink-0 px-5 py-2.5 flex items-center justify-end gap-2 border-t border-line/45 bg-surface-chrome/92 no-drag">
          {footer}
        </footer>
      )}
    </div>
  );
}

function isMacPlatform() {
  if (typeof navigator === "undefined") return true;
  return /mac/i.test(navigator.platform);
}

export function WindowFooterActions({
  onCancel,
  onSubmit,
  submitLabel,
  cancelLabel,
  disabled,
  busy,
}: {
  onCancel: () => void;
  onSubmit: () => void;
  submitLabel?: string;
  cancelLabel?: string;
  disabled?: boolean;
  busy?: boolean;
}) {
  return (
    <>
      <Button variant="ghost" onPress={onCancel}>
        {cancelLabel ?? tm("common.cancel", "Cancel")}
      </Button>
      <Button variant="primary" disabled={disabled} onPress={onSubmit}>
        {busy ? tm("common.processing", "Processing") : (submitLabel ?? tm("common.save", "Save"))}
      </Button>
    </>
  );
}
