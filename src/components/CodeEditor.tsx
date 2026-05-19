import { redo as cmRedo, undo as cmUndo } from "@codemirror/commands";
import { css } from "@codemirror/lang-css";
import { go } from "@codemirror/lang-go";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { openSearchPanel } from "@codemirror/search";
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef } from "react";

export type CodeEditorScrollInfo = {
  ratio: number;
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
};

export type CodeEditorHandle = {
  focus: () => void;
  undo: () => void;
  redo: () => void;
  openSearch: () => void;
  scrollToRatio: (ratio: number) => void;
};

type Props = {
  value: string;
  documentKey: string;
  language: string;
  readOnly?: boolean;
  onChange: (value: string) => void;
  onScrollInfoChange?: (info: CodeEditorScrollInfo) => void;
};

export const CodeEditor = forwardRef<CodeEditorHandle, Props>(function CodeEditor(
  { value, documentKey, language, readOnly = false, onChange, onScrollInfoChange },
  ref,
) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onScrollInfoChangeRef = useRef(onScrollInfoChange);
  const languageExtension = useMemo(() => extensionForLanguage(language), [language]);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    onScrollInfoChangeRef.current = onScrollInfoChange;
  }, [onScrollInfoChange]);

  useImperativeHandle(ref, () => ({
    focus: () => viewRef.current?.focus(),
    undo: () => {
      const view = viewRef.current;
      if (view) cmUndo(view);
    },
    redo: () => {
      const view = viewRef.current;
      if (view) cmRedo(view);
    },
    openSearch: () => {
      const view = viewRef.current;
      if (view) {
        openSearchPanel(view);
        view.focus();
      }
    },
    scrollToRatio: (ratio) => {
      const scrollDOM = viewRef.current?.scrollDOM;
      if (!scrollDOM) return;
      const max = Math.max(0, scrollDOM.scrollHeight - scrollDOM.clientHeight);
      scrollDOM.scrollTop = clamp(ratio, 0, 1) * max;
    },
  }));

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    host.innerHTML = "";

    let view: EditorView | null = null;
    let frame = 0;
    const emitScrollInfo = () => {
      if (!view) return;
      const scrollDOM = view.scrollDOM;
      const max = Math.max(0, scrollDOM.scrollHeight - scrollDOM.clientHeight);
      onScrollInfoChangeRef.current?.({
        ratio: max > 0 ? scrollDOM.scrollTop / max : 0,
        scrollTop: scrollDOM.scrollTop,
        scrollHeight: scrollDOM.scrollHeight,
        clientHeight: scrollDOM.clientHeight,
      });
    };
    const scheduleScrollInfo = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        emitScrollInfo();
      });
    };

    const extensions: Extension[] = [
      basicSetup,
      coduxEditorTheme,
      EditorState.readOnly.of(readOnly),
      EditorView.editable.of(!readOnly),
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
        if (update.docChanged || update.viewportChanged || update.geometryChanged) {
          scheduleScrollInfo();
        }
      }),
    ];
    if (languageExtension) {
      extensions.push(languageExtension);
    }

    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions,
      }),
    });
    viewRef.current = view;
    view.scrollDOM.addEventListener("scroll", scheduleScrollInfo, { passive: true });
    const resizeObserver =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(scheduleScrollInfo);
    resizeObserver?.observe(view.scrollDOM);
    scheduleScrollInfo();
    return () => {
      if (frame) window.cancelAnimationFrame(frame);
      resizeObserver?.disconnect();
      view?.scrollDOM.removeEventListener("scroll", scheduleScrollInfo);
      view.destroy();
      if (viewRef.current === view) {
        viewRef.current = null;
      }
    };
  }, [documentKey, languageExtension, readOnly]);

  return <div ref={hostRef} className="h-full min-h-0 min-w-0 overflow-hidden" />;
});

function extensionForLanguage(language: string): Extension | null {
  switch (language) {
    case "javascript":
      return javascript({ jsx: true, typescript: true });
    case "json":
      return json();
    case "css":
      return css();
    case "html":
      return html();
    case "markdown":
      return markdown();
    case "python":
      return python();
    case "rust":
      return rust();
    case "go":
      return go();
    case "xml":
      return xml();
    case "sql":
      return sql();
    default:
      return null;
  }
}

const coduxEditorTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      minHeight: "0",
      backgroundColor: "var(--surface-editor)",
      color: "var(--color-ink)",
    },
    ".cm-scroller": {
      fontFamily:
        'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
      fontSize: "13px",
      lineHeight: "1.65",
      overflow: "auto",
    },
    ".cm-content": {
      padding: "14px 0 24px",
      caretColor: "var(--color-brand-green)",
      minHeight: "100%",
    },
    ".cm-line": {
      padding: "0 18px 0 8px",
    },
    ".cm-gutters": {
      backgroundColor: "var(--surface-editor)",
      color: "var(--color-ink-faint)",
      borderRight: "1px solid var(--color-line)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      padding: "0 12px 0 14px",
      minWidth: "48px",
    },
    ".cm-activeLine": {
      backgroundColor: "color-mix(in oklab, var(--color-fill) 4%, transparent)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "color-mix(in oklab, var(--color-fill) 5%, transparent)",
      color: "var(--color-ink-mute)",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "color-mix(in oklab, var(--color-brand-blue) 35%, transparent)",
    },
    ".cm-search": {
      backgroundColor: "var(--color-surface-panel)",
      borderColor: "var(--color-line)",
      color: "var(--color-ink)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--color-surface-panel)",
      borderColor: "var(--color-line)",
      color: "var(--color-ink)",
    },
  },
  { dark: true },
);

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
