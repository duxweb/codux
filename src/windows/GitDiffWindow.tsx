import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { ArrowTopRight, FileCode2 } from "../icons";
import type { GitDiffSnapshot } from "../git/status";
import { Button } from "../components/Button";
import { closeCurrentAppWindow, revealCurrentAppWindow } from "../windowing";
import { WindowFrame } from "./WindowFrame";
import { tm } from "../i18n";
import { openFileExternally } from "../files/api";

type DiffRequest = {
  projectPath: string;
  path: string;
  staged: boolean;
};

type DiffLine = {
  id: number;
  kind: "meta" | "file" | "hunk" | "addition" | "deletion" | "context";
  oldLine?: number;
  newLine?: number;
  text: string;
};

export function GitDiffWindow() {
  const request = useMemo(readDiffRequest, []);
  const [snapshot, setSnapshot] = useState<GitDiffSnapshot | null>(null);
  const [isLoading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      if (!request.projectPath || !request.path) {
        setSnapshot({
          path: request.path,
          diff: "",
          isRepository: false,
          error: tm("git.diff.missing_params", "Missing diff parameters."),
        });
        return;
      }
      if (!window.__TAURI_INTERNALS__) {
        setSnapshot({
          path: request.path,
          diff: "",
          isRepository: true,
          error: null,
        });
        return;
      }
      const next = await invoke<GitDiffSnapshot>("git_diff_file", {
        request: {
          projectPath: request.projectPath,
          path: request.path,
          staged: request.staged,
        },
      });
      setSnapshot(next);
      setError(next.error ?? null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load().finally(() => revealCurrentAppWindow());
  }, []);

  const diff = snapshot?.diff || "";
  const openTargetFile = async () => {
    if (!request.projectPath || !request.path) return;
    await openFileExternally(request.projectPath, request.path);
  };

  return (
    <WindowFrame
      title={request.path || "Diff"}
      footer={
        <>
          <Button
            variant="ghost"
            size="sm"
            onPress={() => void closeCurrentAppWindow()}
          >
            {tm("common.close", "Close")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            leading={ArrowTopRight}
            disabled={!request.projectPath || !request.path}
            onPress={() => void openTargetFile()}
          >
            {tm("git.diff.open_file", "Open File")}
          </Button>
        </>
      }
      mainClassName="px-0 py-0"
    >

      {error && (
        <div className="mx-4 mt-3 rounded-md border border-brand-red/30 bg-brand-red/12 px-3 py-2 text-xs text-brand-red">
          {error}
        </div>
      )}

      <main className="min-h-0 flex-1">
        {diff ? (
          <UnifiedDiffView diff={diff} />
        ) : (
          <div className="h-full grid place-items-center bg-surface-editor text-center px-6">
            <div>
              <div className="w-11 h-11 mx-auto rounded-[10px] border border-line bg-fill/[0.04] grid place-items-center text-ink-mute">
                <FileCode2 size={18} />
              </div>
              <div className="mt-3 text-sm font-semibold text-ink">
                {isLoading ? tm("git.diff.loading", "Loading diff...") : tm("git.diff.empty", "No diff to display")}
              </div>
              <div className="mt-1 text-xs text-ink-mute max-w-[280px] leading-relaxed">
                {snapshot?.error || tm("git.diff.empty_description", "This file did not produce diff content.")}
              </div>
            </div>
          </div>
        )}
      </main>
    </WindowFrame>
  );
}

function UnifiedDiffView({ diff }: { diff: string }) {
  const lines = useMemo(() => parseUnifiedDiff(diff), [diff]);
  return (
    <div className="h-full min-h-0 overflow-auto scrollbar-overlay bg-surface-editor text-[12px] font-mono leading-[1.55]">
      <div className="min-w-max py-2">
        {lines.map((line) => (
          <div
            key={line.id}
            className={`grid grid-cols-[54px_54px_max-content] ${
              line.kind === "addition"
                ? "bg-brand-green/10 text-ink"
                : line.kind === "deletion"
                  ? "bg-brand-red/10 text-ink"
                  : line.kind === "hunk"
                    ? "bg-brand-blue/12 text-brand-blue"
                    : line.kind === "file"
                      ? "bg-brand-amber/10 text-brand-amber"
                      : "border-transparent text-ink-soft"
            }`}
          >
            <span className="select-none border-r border-line/50 px-2 text-right text-ink-faint">
              {line.oldLine ?? ""}
            </span>
            <span className="select-none border-r border-line/50 px-2 text-right text-ink-faint">
              {line.newLine ?? ""}
            </span>
            <code className="whitespace-pre px-3">{line.text || " "}</code>
          </div>
        ))}
      </div>
    </div>
  );
}

function parseUnifiedDiff(diff: string): DiffLine[] {
  let oldLine: number | undefined;
  let newLine: number | undefined;
  return diff.split(/\r?\n/).map((text, id) => {
    const hunk = text.match(/^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/);
    if (hunk) {
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      return { id, kind: "hunk", text };
    }
    if (
      text.startsWith("diff --git") ||
      text.startsWith("index ") ||
      text.startsWith("new file mode") ||
      text.startsWith("deleted file mode")
    ) {
      return { id, kind: "meta", text };
    }
    if (text.startsWith("--- ") || text.startsWith("+++ ")) {
      return { id, kind: "file", text };
    }
    if (text.startsWith("+")) {
      const line = { id, kind: "addition" as const, newLine, text };
      if (newLine !== undefined) newLine += 1;
      return line;
    }
    if (text.startsWith("-")) {
      const line = { id, kind: "deletion" as const, oldLine, text };
      if (oldLine !== undefined) oldLine += 1;
      return line;
    }
    const line = { id, kind: "context" as const, oldLine, newLine, text };
    if (oldLine !== undefined) oldLine += 1;
    if (newLine !== undefined) newLine += 1;
    return line;
  });
}

function readDiffRequest(): DiffRequest {
  const params = new URLSearchParams(window.location.hash.split("?")[1] ?? "");
  return {
    projectPath: params.get("projectPath") ?? "",
    path: params.get("path") ?? "",
    staged: params.get("staged") === "1",
  };
}
