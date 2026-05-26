import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createRoot, type Root } from "react-dom/client";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "./components/Button";
import { ProgressBar, Spinner } from "./components/Feedback";
import { Modal } from "./components/Modal";
import { formatI18n, tm } from "./i18n";
import { CheckCircle2, RefreshCw } from "./icons";
import type { UpdateInstallProgressEvent, UpdateInstallResult, UpdateStatus } from "./appActions";

type DialogPhase =
  | "checking"
  | "available"
  | "latest"
  | "notConfigured"
  | "downloading"
  | "installing"
  | "installed"
  | "error";

const fallbackStatus: UpdateStatus = {
  configured: false,
  checking: false,
  available: false,
  automaticInstallSupported: false,
  signedUpdaterConfigured: false,
  manifestEndpointConfigured: false,
  currentVersion: "0.1.0",
  latestVersion: null,
  downloadUrl: null,
  notes: null,
  channel: null,
  installationMode: "preview",
  message: tm("update.not_configured.preview", "Update channel is not configured in browser preview."),
};

let activeRoot: Root | null = null;
let activeHost: HTMLDivElement | null = null;

export async function showUpdateDialog() {
  if (typeof document === "undefined") return;
  if (activeRoot && activeHost) {
    activeRoot.render(<UpdateDialog onClose={closeUpdateDialog} />);
    return;
  }
  activeHost = document.createElement("div");
  activeHost.className = "no-drag";
  document.body.appendChild(activeHost);
  activeRoot = createRoot(activeHost);
  activeRoot.render(<UpdateDialog onClose={closeUpdateDialog} />);
}

function closeUpdateDialog() {
  const root = activeRoot;
  const host = activeHost;
  activeRoot = null;
  activeHost = null;
  root?.unmount();
  host?.remove();
}

async function openUrl(url: string) {
  if (window.__TAURI_INTERNALS__) {
    await invoke("app_open_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

function UpdateDialog({ onClose }: { onClose: () => void }) {
  const [phase, setPhase] = useState<DialogPhase>("checking");
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [progress, setProgress] = useState<UpdateInstallProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<UpdateInstallResult | null>(null);

  const loadStatus = useCallback(async () => {
    setPhase("checking");
    setError(null);
    setResult(null);
    setProgress(null);
    try {
      const next = window.__TAURI_INTERNALS__ ? await invoke<UpdateStatus>("app_update_status") : fallbackStatus;
      setStatus(next);
      if (!next.configured) {
        setPhase("notConfigured");
      } else if (next.available) {
        setPhase("available");
      } else {
        setPhase("latest");
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setPhase("error");
    }
  }, []);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  const install = useCallback(async () => {
    if (!status) return;
    if (!status.automaticInstallSupported || !window.__TAURI_INTERNALS__) {
      if (status.downloadUrl) await openUrl(status.downloadUrl);
      onClose();
      return;
    }
    setPhase("downloading");
    setProgress({
      phase: "downloading",
      version: status.latestVersion,
      downloadedBytes: 0,
      totalBytes: null,
    });
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<UpdateInstallProgressEvent>("app:update-install-progress", ({ payload }) => {
        setProgress(payload);
        if (payload.phase === "installing") setPhase("installing");
        if (payload.phase === "finished") setPhase("installed");
      });
      const nextResult = await invoke<UpdateInstallResult>("app_update_install");
      setResult(nextResult);
      setPhase(nextResult.installed ? "installed" : "latest");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setPhase("error");
    } finally {
      unlisten?.();
    }
  }, [onClose, status]);

  const restart = useCallback(() => {
    if (!window.__TAURI_INTERNALS__) {
      window.location.reload();
      return;
    }
    void invoke("app_request_restart");
  }, []);

  const canClose = phase !== "downloading" && phase !== "installing";

  return (
    <Modal isOpen onOpenChange={(isOpen) => (!isOpen && canClose ? onClose() : undefined)}>
      <Modal.Backdrop className="no-drag fixed inset-0 z-[9800] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
        <Modal.Container size="md" placement="center">
          <Modal.Dialog className="no-drag w-[min(420px,calc(100vw-32px))] rounded-[12px] border border-border bg-surface-main px-5 py-4 text-ink shadow-floating outline-none">
            <Modal.Header className="mb-4 p-0">
              <div className="flex min-w-0 items-center gap-3">
                <UpdateStatusIcon phase={phase} />
                <div className="min-w-0 flex-1">
                  <Modal.Heading className="text-[15px] font-semibold leading-5 text-ink">
                    {titleForPhase(phase)}
                  </Modal.Heading>
                  {subtitleForPhase(phase, status, progress) ? (
                    <div className="mt-1 text-sm leading-5 text-ink-faint">
                      {subtitleForPhase(phase, status, progress)}
                    </div>
                  ) : null}
                </div>
              </div>
            </Modal.Header>

            <UpdateDialogBody
              phase={phase}
              status={status}
              progress={progress}
              result={result}
              error={error}
            />

            <Modal.Footer className="mt-5 flex justify-end gap-2 p-0">
              {phase === "checking" || phase === "downloading" || phase === "installing" ? null : (
                <Button size="sm" variant="secondary" onPress={onClose}>
                  {phase === "installed" ? tm("common.later", "Later") : tm("common.close", "Close")}
                </Button>
              )}
              {phase === "available" && status ? (
                <Button size="sm" variant="primary" onPress={() => void install()}>
                  {status.automaticInstallSupported
                    ? tm("update.available.install", "Install")
                    : tm("update.available.open", "Download")}
                </Button>
              ) : null}
              {phase === "installed" ? (
                <Button size="sm" variant="primary" onPress={restart}>
                  {tm("common.restart_now", "Restart Now")}
                </Button>
              ) : null}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function UpdateDialogBody({
  phase,
  status,
  progress,
  result,
  error,
}: {
  phase: DialogPhase;
  status: UpdateStatus | null;
  progress: UpdateInstallProgressEvent | null;
  result: UpdateInstallResult | null;
  error: string | null;
}) {
  if (phase === "checking") {
    return (
      <p className="text-sm leading-6 text-ink-soft">
        {tm("update.checking", "Checking for updates...")}
      </p>
    );
  }

  if (phase === "downloading" || phase === "installing") {
    return <UpdateProgress phase={phase} progress={progress} />;
  }

  if (phase === "installed") {
    return (
      <div className="grid gap-3">
        <p className="text-sm leading-relaxed text-ink-soft">
          {tm("update.installed.message", result?.message ?? "The update was downloaded and installed. Restart Codux to finish applying it.")}
        </p>
        <UpdateProgress phase="installed" progress={progress} />
      </div>
    );
  }

  if (phase === "error") {
    return (
      <p className="text-sm leading-6 text-brand-red">
        {error ?? tm("update.error.message", "Please check your network connection and try again.")}
      </p>
    );
  }

  if (!status) return null;

  if (phase === "notConfigured") {
    return <p className="text-sm leading-6 text-ink-soft">{status.message}</p>;
  }

  if (phase === "latest") {
    return <p className="text-sm leading-6 text-ink-soft">{tm("update.latest.message", "No new version found.")}</p>;
  }

  return (
    <ReleaseNotes notes={status.notes} />
  );
}

function ReleaseNotes({ notes }: { notes?: string | null }) {
  return (
    <div className="max-h-[220px] overflow-y-auto whitespace-pre-wrap pr-1 text-sm leading-6 text-ink-soft scrollbar-overlay">
      {notes?.trim() || tm("update.release_notes.empty", "No release notes were provided for this update.")}
    </div>
  );
}

function UpdateProgress({
  phase,
  progress,
}: {
  phase: "downloading" | "installing" | "installed";
  progress: UpdateInstallProgressEvent | null;
}) {
  const percentage = useMemo(() => {
    const total = progress?.totalBytes ?? 0;
    if (total <= 0) return null;
    return Math.min(100, Math.max(0, ((progress?.downloadedBytes ?? 0) / total) * 100));
  }, [progress?.downloadedBytes, progress?.totalBytes]);
  const label =
    phase === "installing"
      ? tm("update.progress.installing", "Installing update...")
      : phase === "installed"
        ? tm("update.progress.finished", "Installation is complete.")
        : tm("update.progress.downloading", "Downloading update...");

  return (
    <div className="grid gap-2.5">
      <p className="text-sm leading-6 text-ink-soft">{label}</p>
      <ProgressBar value={percentage ?? undefined} isIndeterminate={percentage === null && phase !== "installed"}>
        <ProgressBar.Track className="h-1.5 overflow-hidden rounded-full bg-fill/14">
          <ProgressBar.Fill className="h-full rounded-full bg-brand-blue" />
        </ProgressBar.Track>
      </ProgressBar>
      <div className="text-xs tabular-nums leading-4 text-ink-faint">
        {progress?.totalBytes
          ? formatI18n(
              tm("update.progress.bytes_format", "%@ of %@"),
              formatBytes(progress.downloadedBytes),
              formatBytes(progress.totalBytes),
            )
          : formatBytes(progress?.downloadedBytes ?? 0)}
      </div>
    </div>
  );
}

function titleForPhase(phase: DialogPhase) {
  switch (phase) {
    case "available":
      return tm("update.available.title", "Update Available");
    case "latest":
      return tm("update.latest.title", "Up to Date");
    case "notConfigured":
      return tm("update.not_configured.title", "Updates Not Configured");
    case "downloading":
    case "installing":
      return tm("update.progress.title", "Installing Update");
    case "installed":
      return tm("update.installed.title", "Update Installed");
    case "error":
      return tm("update.error.title", "Unable to Check for Updates");
    case "checking":
    default:
      return tm("about.updates", "Check for Updates");
  }
}

function UpdateStatusIcon({ phase }: { phase: DialogPhase }) {
  const isBusy = phase === "checking" || phase === "downloading" || phase === "installing";
  const isSuccess = phase === "latest" || phase === "installed";
  const className = isSuccess
    ? "bg-brand-green/12 text-brand-green"
    : phase === "error"
      ? "bg-brand-red/12 text-brand-red"
      : "bg-brand-blue/12 text-brand-blue";

  return (
    <span className={`grid h-8 w-8 flex-none place-items-center rounded-[9px] ${className}`}>
      {isBusy ? (
        <Spinner size="sm" />
      ) : isSuccess ? (
        <CheckCircle2 size={17} strokeWidth={2.2} />
      ) : (
        <RefreshCw size={16} strokeWidth={2.2} />
      )}
    </span>
  );
}

function subtitleForPhase(
  phase: DialogPhase,
  status: UpdateStatus | null,
  progress: UpdateInstallProgressEvent | null,
) {
  if (status && phase === "available") {
    return formatI18n(
      tm("update.version.summary_format", "Current v%@ · Latest v%@"),
      status.currentVersion,
      status.latestVersion ?? status.currentVersion,
    );
  }
  if (phase === "downloading" || phase === "installing" || phase === "installed") {
    return progress?.version
      ? formatI18n(tm("update.progress.version_format", "Version v%@"), progress.version)
      : tm("update.progress.title", "Installing Update");
  }
  return null;
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}
