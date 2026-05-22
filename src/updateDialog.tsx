import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Modal, ProgressBar, Spinner } from "@heroui/react";
import { createRoot, type Root } from "react-dom/client";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "./components/Button";
import { formatI18n, tm } from "./i18n";
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
  const progressPhase = phase === "downloading" || phase === "installing" || phase === "installed";

  if (progressPhase) {
    return (
      <UpdateProgressDialog
        phase={phase}
        progress={progress}
        result={result}
        error={error}
        onClose={onClose}
        onRestart={restart}
      />
    );
  }

  return (
    <Modal isOpen onOpenChange={(isOpen) => (!isOpen && canClose ? onClose() : undefined)}>
      <Modal.Backdrop className="no-drag fixed inset-0 z-[9800] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
        <Modal.Container size="md" placement="center">
          <Modal.Dialog className="no-drag w-[min(540px,calc(100vw-32px))] rounded-[12px] border border-line-strong bg-surface-chrome p-4 text-ink shadow-pop outline-none">
            <Modal.Header className="mb-3 p-0">
              <div className="min-w-0">
                <Modal.Heading className="text-sm font-semibold text-ink">{titleForPhase(phase)}</Modal.Heading>
                {status ? (
                  <div className="mt-1 text-sm text-ink-faint">
                    {formatI18n(
                      tm("update.version.summary_format", "Current v%@ · Latest v%@"),
                      status.currentVersion,
                      status.latestVersion ?? status.currentVersion,
                    )}
                  </div>
                ) : null}
              </div>
            </Modal.Header>

            <UpdateDialogBody
              phase={phase}
              status={status}
              progress={progress}
              result={result}
              error={error}
            />

            <Modal.Footer className="mt-4 flex justify-end gap-2 p-0">
              {phase === "checking" ? null : (
                <Button size="sm" variant="ghost" onPress={onClose}>
                  {tm("common.close", "Close")}
                </Button>
              )}
              {phase === "available" && status ? (
                <Button size="sm" variant="primary" onPress={() => void install()}>
                  {status.automaticInstallSupported
                    ? tm("update.available.install", "Install")
                    : tm("update.available.open", "Download")}
                </Button>
              ) : null}
              {phase === "latest" || phase === "notConfigured" || phase === "error" ? (
                <Button size="sm" variant="secondary" onPress={() => void loadStatus()}>
                  {tm("common.refresh", "Refresh")}
                </Button>
              ) : null}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function UpdateProgressDialog({
  phase,
  progress,
  result,
  error,
  onClose,
  onRestart,
}: {
  phase: "downloading" | "installing" | "installed";
  progress: UpdateInstallProgressEvent | null;
  result: UpdateInstallResult | null;
  error: string | null;
  onClose: () => void;
  onRestart: () => void;
}) {
  const canClose = phase === "installed";

  return (
    <Modal isOpen onOpenChange={(isOpen) => (!isOpen && canClose ? onClose() : undefined)}>
      <Modal.Backdrop className="no-drag fixed inset-0 z-[9800] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
        <Modal.Container size="sm" placement="center">
          <Modal.Dialog className="no-drag w-[min(440px,calc(100vw-32px))] rounded-[12px] border border-line-strong bg-surface-chrome p-4 text-ink shadow-pop outline-none">
            <Modal.Header className="mb-3 p-0">
              <div className="min-w-0">
                <Modal.Heading className="text-sm font-semibold text-ink">
                  {titleForPhase(error ? "error" : phase)}
                </Modal.Heading>
                <div className="mt-1 text-sm text-ink-faint">
                  {progress?.version
                    ? formatI18n(tm("update.progress.version_format", "Version v%@"), progress.version)
                    : tm("update.progress.title", "Installing Update")}
                </div>
              </div>
            </Modal.Header>
            {error ? (
              <div className="rounded-md border border-brand-red/25 bg-brand-red/10 px-3 py-2.5 text-sm leading-relaxed text-brand-red">
                {error}
              </div>
            ) : (
              <UpdateDialogBody
                phase={phase}
                status={null}
                progress={progress}
                result={result}
                error={null}
              />
            )}
            <Modal.Footer className="mt-4 flex justify-end gap-2 p-0">
              {phase === "installed" || error ? (
                <Button size="sm" variant="ghost" onPress={onClose}>
                  {tm("common.later", "Later")}
                </Button>
              ) : null}
              {phase === "installed" ? (
                <Button size="sm" variant="primary" onPress={onRestart}>
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
      <div className="flex min-h-[120px] items-center justify-center gap-3 text-sm text-ink-soft">
        <Spinner size="sm" />
        <span>{tm("update.checking", "Checking for updates...")}</span>
      </div>
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
      <div className="rounded-md border border-brand-red/25 bg-brand-red/10 px-3 py-2.5 text-sm leading-relaxed text-brand-red">
        {error ?? tm("update.error.message", "Please check your network connection and try again.")}
      </div>
    );
  }

  if (!status) return null;

  if (phase === "notConfigured") {
    return <p className="text-sm leading-relaxed text-ink-soft">{status.message}</p>;
  }

  if (phase === "latest") {
    return (
      <p className="text-sm leading-relaxed text-ink-soft">
        {formatI18n(
          tm("update.latest.message_format", "Current version: v%@\nLatest release: v%@"),
          status.currentVersion,
          status.latestVersion ?? status.currentVersion,
        )}
      </p>
    );
  }

  return (
    <div className="grid gap-3">
      <p className="text-sm leading-relaxed text-ink-soft">
        {formatI18n(
          tm("update.available.message_format", "A new version v%@ is available. You are currently using v%@."),
          status.latestVersion ?? status.currentVersion,
          status.currentVersion,
        )}
      </p>
      <div>
        <div className="mb-1.5 text-sm font-semibold text-ink-soft">
          {tm("update.available.notes_title", "Release Notes")}
        </div>
        <ReleaseNotes notes={status.notes} />
      </div>
    </div>
  );
}

function ReleaseNotes({ notes }: { notes?: string | null }) {
  return (
    <div className="max-h-[220px] min-h-[132px] overflow-y-auto whitespace-pre-wrap rounded-md border border-line bg-fill/[0.035] px-3 py-2.5 text-sm leading-relaxed text-ink-soft">
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
    <div className="grid gap-2.5 rounded-md border border-line bg-fill/[0.035] px-3 py-3">
      <div className="flex items-center justify-between gap-3 text-sm">
        <span className="font-medium text-ink-soft">{label}</span>
        <span className="text-xs tabular-nums text-ink-faint">
          {percentage === null ? formatBytes(progress?.downloadedBytes ?? 0) : `${Math.round(percentage)}%`}
        </span>
      </div>
      <ProgressBar value={percentage ?? undefined} isIndeterminate={percentage === null && phase !== "installed"}>
        <ProgressBar.Track className="h-1.5 overflow-hidden rounded-full bg-fill/12">
          <ProgressBar.Fill className="h-full rounded-full bg-brand-blue" />
        </ProgressBar.Track>
      </ProgressBar>
      <div className="text-xs tabular-nums text-ink-faint">
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
      return tm("update.latest.title", "You're up to date.");
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
