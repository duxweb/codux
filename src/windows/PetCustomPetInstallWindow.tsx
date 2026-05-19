import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowTopRight, CheckCircle2, Info, Search } from "../icons";
import { emitPetCustomPetInstalled } from "../ai/petCustomEvents";
import {
  installCustomPet,
  previewCustomPetInstall,
  type PetCustomPetInstallPreview,
} from "../ai/petState";
import { openExternalUrl } from "../appActions";
import { Button } from "../components/Button";
import { TextInput } from "../components/Form";
import { formatI18n, tm } from "../i18n";
import { systemMessage } from "../systemDialog";
import { closeCurrentAppWindow, revealCurrentAppWindow } from "../windowing";
import { WindowFrame } from "./WindowFrame";

const WINDOW_WIDTH = 680;
const WINDOW_MIN_HEIGHT = 210;
const WINDOW_HEADER_HEIGHT = 40;
const WINDOW_FOOTER_HEIGHT = 62;
const WINDOW_MAIN_VERTICAL_PADDING = 40;
const WINDOW_VERTICAL_INSET = WINDOW_HEADER_HEIGHT + WINDOW_FOOTER_HEIGHT + WINDOW_MAIN_VERTICAL_PADDING;
const WINDOW_MAX_HEIGHT = 640;

export function PetCustomPetInstallWindow() {
  const contentRef = useRef<HTMLDivElement | null>(null);
  const lastWindowHeightRef = useRef(0);
  const [pageUrl, setPageUrl] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [preview, setPreview] = useState<PetCustomPetInstallPreview | null>(null);
  const [phase, setPhase] = useState<"idle" | "resolving" | "ready" | "installing">("idle");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void revealCurrentAppWindow();
  }, []);

  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!content || !window.__TAURI_INTERNALS__) return;
    const resize = () => {
      const contentHeight = content.scrollHeight;
      const nextHeight = Math.min(
        WINDOW_MAX_HEIGHT,
        Math.max(WINDOW_MIN_HEIGHT, Math.ceil(contentHeight + WINDOW_VERTICAL_INSET)),
      );
      if (Math.abs(lastWindowHeightRef.current - nextHeight) < 2) return;
      lastWindowHeightRef.current = nextHeight;
      void getCurrentWindow()
        .setSize(new LogicalSize(WINDOW_WIDTH, nextHeight))
        .catch((reason) => console.error("failed to resize custom pet install window", reason));
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(content);
    return () => observer.disconnect();
  }, [error, phase, preview]);

  const trimmedUrl = pageUrl.trim();
  const isBusy = phase === "resolving" || phase === "installing";
  const hostLabel = useMemo(() => {
    if (!preview?.pageUrl) return "petdex.crafter.run";
    try {
      return new URL(preview.pageUrl).host;
    } catch {
      return "petdex.crafter.run";
    }
  }, [preview?.pageUrl]);

  const resolve = async () => {
    if (!trimmedUrl || isBusy) return;
    setPhase("resolving");
    setError(null);
    try {
      const next = await previewCustomPetInstall(trimmedUrl, displayName);
      setPreview(next);
      setDisplayName(next.displayName);
      setPhase("ready");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setPhase("idle");
    }
  };

  const install = async () => {
    if (!preview || !displayName.trim() || phase === "installing") return;
    setPhase("installing");
    setError(null);
    try {
      const installed = await installCustomPet(trimmedUrl, displayName);
      await emitPetCustomPetInstalled(installed);
      await systemMessage(formatI18n(tm("pet.custom.install.success_format", "Installed %@."), installed.displayName), {
        title: tm("pet.custom.install.title", "Add Custom Pet"),
        kind: "info",
        buttons: { ok: "OK" },
      });
      await closeCurrentAppWindow();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setPhase("ready");
    }
  };

  const openMarket = () => {
    void openExternalUrl(tm("pet.custom.market.url", "https://petdex.crafter.run"));
  };

  return (
    <WindowFrame
      title={tm("pet.custom.install.title", "Add Custom Pet")}
      footer={
        <>
          <Button variant="ghost" disabled={isBusy} onPressUp={() => void closeCurrentAppWindow()}>
            {tm("common.cancel", "Cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!preview || !displayName.trim() || phase === "installing"}
            onPress={() => void install()}
          >
            {phase === "installing" ? tm("pet.custom.install.installing", "Installing...") : tm("pet.custom.install.confirm", "Install")}
          </Button>
        </>
      }
    >
      <div ref={contentRef} className="grid gap-4">
        <label className="grid gap-2">
          <span className="text-xs font-semibold text-ink-mute">
            {tm("pet.custom.install.url.label", "Petdex Page URL")}
          </span>
          <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] gap-2">
            <TextInput
              value={pageUrl}
              disabled={isBusy}
              onChange={(event) => {
                setPageUrl(event.currentTarget.value);
                setPreview(null);
                setError(null);
                setPhase("idle");
              }}
              placeholder={tm("pet.custom.install.url.placeholder", "https://petdex.crafter.run/zh/pets/boba")}
              autoFocus
            />
            <Button
              variant="ghost"
              leading={ArrowTopRight}
              disabled={isBusy}
              onPress={openMarket}
            >
              {tm("pet.custom.market.action", "Get Pets")}
            </Button>
            <Button
              variant="secondary"
              leading={Search}
              disabled={!trimmedUrl || phase === "resolving" || phase === "installing"}
              onPress={() => void resolve()}
            >
              {phase === "resolving"
                ? tm("pet.custom.install.resolving", "Reading Petdex page...")
                : preview
                  ? tm("pet.custom.install.resolve_again", "Parse Again")
                  : tm("pet.custom.install.resolve", "Parse")}
            </Button>
          </div>
        </label>

        {preview && (
          <>
            <div className="grid grid-cols-[104px_minmax(0,1fr)] gap-4 rounded-[12px] border border-line bg-fill/[0.035] p-4">
              <div className="grid h-[104px] w-[104px] place-items-center overflow-hidden rounded-[12px] bg-brand-blue/10">
                {preview.imageUrl ? (
                  <img
                    src={preview.imageUrl}
                    alt=""
                    className="h-full w-full object-cover"
                    draggable={false}
                  />
                ) : (
                  <Info size={34} className="text-brand-blue" />
                )}
              </div>
              <div className="min-w-0 self-center">
                <div className="truncate text-[17px] font-bold">{preview.displayName}</div>
                {preview.description && (
                  <div className="mt-1 max-h-[72px] overflow-y-auto scrollbar-overlay text-xs leading-5 text-ink-mute">
                    {preview.description}
                  </div>
                )}
                <div className="mt-2 flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-ink-faint">
                  <ArrowTopRight size={12} strokeWidth={2} />
                  <span className="truncate">{hostLabel}</span>
                </div>
              </div>
            </div>

            <label className="grid gap-2">
              <span className="text-xs font-semibold text-ink-mute">
                {tm("pet.custom.install.name.label", "Pet Name")}
              </span>
              <TextInput
                value={displayName}
                disabled={isBusy}
                onChange={(event) => setDisplayName(event.currentTarget.value)}
                placeholder={preview.displayName}
              />
            </label>

            <div className="grid gap-2 text-xs text-ink-soft">
              <InstallCheck text={tm("pet.custom.install.validation.page", "Petdex page verified")} />
              <InstallCheck text={tm("pet.custom.install.validation.package", "Package link found")} />
              <InstallCheck text={tm("pet.custom.install.validation.format", "Codex-format check runs during install")} />
            </div>
          </>
        )}

        {phase === "installing" && (
          <div className="rounded-[9px] bg-brand-blue/10 px-3 py-2 text-xs font-medium text-brand-blue">
            {tm("pet.custom.install.installing.detail", "Downloading, unpacking, and validating the pet package.")}
          </div>
        )}

        {error && <div className="text-xs font-medium text-brand-red">{error}</div>}
      </div>
    </WindowFrame>
  );
}

function InstallCheck({ text }: { text: string }) {
  return (
    <div className="flex items-center gap-2">
      <CheckCircle2 size={13} className="text-brand-green" />
      <span>{text}</span>
    </div>
  );
}
