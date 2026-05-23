import { invoke } from "@tauri-apps/api/core";
import { Modal } from "@heroui/react";
import { useEffect, useState } from "react";
import { checkForUpdates, openExternalUrl, type AppAboutMetadata } from "../appActions";
import { AppIconMark } from "../components/AppIconMark";
import { Button } from "../components/Button";
import { CODUX_WEBSITE_URL } from "../appLinks";
import { tm } from "../i18n";
import { readAppSettings, subscribeAppSettings } from "../settings";
import { WindowFrame } from "./WindowFrame";

const fallbackAbout: AppAboutMetadata = {
  name: "Codux",
  version: "0.1.0",
  identifier: "com.duxweb.dmux",
  description: "Codux desktop workspace",
  targetOs: "web",
  targetArch: "browser",
  buildProfile: "preview",
};

export function AboutWindow() {
  const [about, setAbout] = useState<AppAboutMetadata>(fallbackAbout);
  const [isChecking, setChecking] = useState(false);
  const [isAgreementOpen, setAgreementOpen] = useState(false);
  const [iconStyle, setIconStyle] = useState(() => readAppSettings().iconStyle);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    void invoke<AppAboutMetadata>("app_about_metadata")
      .then(setAbout)
      .catch((error) => console.error("failed to load about metadata", error));
  }, []);
  useEffect(() => subscribeAppSettings((settings) => setIconStyle(settings.iconStyle)), []);

  const runUpdateCheck = async () => {
    setChecking(true);
    try {
      await checkForUpdates();
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <WindowFrame
        title={tm("menu.app.about_format", "About %@").replace("%@", about.name)}
        mainScrollable={false}
        mainClassName="px-0 py-0"
      >
        <div className="flex h-full flex-col items-center px-6 text-center">
          <div className="h-6 flex-shrink-0" />
          <AppIconMark styleName={iconStyle} size={96} />
          <div className="mt-3.5 text-[20px] font-bold leading-tight">{about.name}</div>
          <div className="mt-1 text-xs text-ink-soft">
            {about.version} · {about.targetOs}/{about.targetArch} · {about.buildProfile}
          </div>
          <div className="mt-5 grid gap-1">
            <div className="text-xs text-ink-soft">{tm("about.tagline", "AI-Powered Terminal Workspace")}</div>
            <div className="text-[11px] text-ink-faint">
              {tm("about.copyright", "Copyright (c) 2025 Codux contributors")}
            </div>
          </div>
          <div className="mt-5 flex items-center justify-center gap-3">
            <Button size="sm" variant="secondary" onPress={() => setAgreementOpen(true)}>
              {tm("about.agreement", "Agreement")}
            </Button>
            <Button size="sm" variant="secondary" onPress={() => void openExternalUrl(CODUX_WEBSITE_URL)}>
              {tm("about.website", "Website")}
            </Button>
            <Button size="sm" variant="secondary" disabled={isChecking} onPress={() => void runUpdateCheck()}>
              {isChecking ? tm("about.checking_updates", "Checking...") : tm("about.updates", "Updates")}
            </Button>
          </div>
          <div className="mt-5 max-w-[260px] truncate text-[10.5px] text-ink-faint">{about.identifier}</div>
        </div>
      </WindowFrame>
      <AgreementModal isOpen={isAgreementOpen} onClose={() => setAgreementOpen(false)} />
    </>
  );
}

function AgreementModal({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const agreementSections = [
    tm(
      "about.user_agreement_body",
      "By using it, you understand that terminal, Git, and AI activity features read local project metadata and runtime state, but do not proactively upload your project contents. You are responsible for the safety of your local environment, permissions, third-party CLIs, and repository credentials. Continued use means you accept that this experimental software may change behavior, interface, and compatibility over time.",
    ),
    tm(
      "about.user_agreement_data",
      "Codux only reads the local state needed to display terminal sessions, Git repository status, AI tool activity, and local statistics. You are responsible for reviewing any third-party CLI behavior and network activity triggered by those tools.",
    ),
    tm(
      "about.user_agreement_responsibility",
      "You are responsible for your local environment, file permissions, repository credentials, notification permissions, and any commands executed inside the terminal.",
    ),
    tm(
      "about.user_agreement_license",
      "Codux is distributed as open-source software under the GPL-3.0 license. Continued use means you accept that this experimental software may change behavior, interface, and compatibility over time.",
    ),
  ];

  return (
    <Modal isOpen={isOpen} onOpenChange={(open) => (!open ? onClose() : undefined)}>
      <Modal.Backdrop className="no-drag fixed inset-0 z-[9000] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
        <Modal.Container size="md" placement="center">
          <Modal.Dialog className="no-drag flex max-h-[min(520px,calc(100vh-48px))] w-[min(520px,calc(100vw-32px))] flex-col rounded-[12px] border border-border bg-surface-main text-ink shadow-floating outline-none">
            <Modal.Header className="flex-shrink-0 border-b border-border-subtle/70 px-4 py-3">
              <Modal.Heading className="text-sm font-semibold text-ink">
                {tm("about.user_agreement", "User Agreement")}
              </Modal.Heading>
            </Modal.Header>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3 text-left text-sm leading-relaxed text-ink-soft scrollbar-overlay">
              <div className="grid gap-3">
                {agreementSections.map((section, index) => (
                  <p key={index}>{section}</p>
                ))}
              </div>
            </div>
            <Modal.Footer className="flex flex-shrink-0 justify-end border-t border-border-subtle/70 px-4 py-3">
              <Button size="sm" variant="primary" onPress={onClose}>
                {tm("common.ok", "OK")}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
