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
import { yaml } from "@codemirror/lang-yaml";
import { HighlightStyle, StreamLanguage, syntaxHighlighting } from "@codemirror/language";
import { c, cpp, csharp, dart, java, kotlin } from "@codemirror/legacy-modes/mode/clike";
import { diff } from "@codemirror/legacy-modes/mode/diff";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { r } from "@codemirror/legacy-modes/mode/r";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { swift } from "@codemirror/legacy-modes/mode/swift";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";
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
  scrollToTop: (scrollTop: number) => void;
};

type Props = {
  value: string;
  documentKey: string;
  language: string;
  readOnly?: boolean;
  onChange: (value: string) => void;
  onScrollInfoChange?: (info: CodeEditorScrollInfo) => void;
  initialScrollTop?: number;
};

export const CodeEditor = forwardRef<CodeEditorHandle, Props>(function CodeEditor(
  { value, documentKey, language, readOnly = false, onChange, onScrollInfoChange, initialScrollTop = 0 },
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
    scrollToTop: (scrollTop) => {
      const scrollDOM = viewRef.current?.scrollDOM;
      if (!scrollDOM) return;
      scrollDOM.scrollTop = Math.max(0, scrollTop);
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
      syntaxHighlighting(coduxHighlightStyle),
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
    if (initialScrollTop > 0) {
      window.requestAnimationFrame(() => {
        if (!view) return;
        view.scrollDOM.scrollTop = initialScrollTop;
        scheduleScrollInfo();
      });
    } else {
      scheduleScrollInfo();
    }
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
    case "yaml":
      return yaml();
    case "toml":
      return StreamLanguage.define(toml);
    case "properties":
      return StreamLanguage.define(properties);
    case "shell":
      return StreamLanguage.define(shell);
    case "dockerfile":
      return StreamLanguage.define(dockerFile);
    case "diff":
      return StreamLanguage.define(diff);
    case "ruby":
      return StreamLanguage.define(ruby);
    case "java":
      return StreamLanguage.define(java);
    case "kotlin":
      return StreamLanguage.define(kotlin);
    case "swift":
      return StreamLanguage.define(swift);
    case "c":
      return StreamLanguage.define(c);
    case "cpp":
      return StreamLanguage.define(cpp);
    case "csharp":
      return StreamLanguage.define(csharp);
    case "dart":
      return StreamLanguage.define(dart);
    case "lua":
      return StreamLanguage.define(lua);
    case "r":
      return StreamLanguage.define(r);
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
      caretColor: "var(--terminal-cursor)",
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
      backgroundColor: "color-mix(in oklab, var(--terminal-selection) 22%, transparent)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "color-mix(in oklab, var(--terminal-selection) 28%, transparent)",
      color: "var(--color-ink-mute)",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "color-mix(in oklab, var(--terminal-selection) 72%, transparent)",
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

const coduxHighlightStyle = HighlightStyle.define([
  { tag: [tags.comment, tags.lineComment, tags.blockComment, tags.docComment], color: "var(--editor-comment)", fontStyle: "italic" },
  { tag: [tags.keyword, tags.controlKeyword, tags.definitionKeyword, tags.moduleKeyword, tags.modifier], color: "var(--editor-keyword)" },
  { tag: [tags.atom, tags.bool, tags.null, tags.self], color: "var(--editor-atom)" },
  { tag: [tags.string, tags.docString, tags.character, tags.attributeValue], color: "var(--editor-string)" },
  { tag: [tags.regexp, tags.escape, tags.special(tags.string)], color: "var(--editor-string2)" },
  { tag: [tags.number, tags.integer, tags.float], color: "var(--editor-number)" },
  { tag: [tags.variableName, tags.name, tags.labelName], color: "var(--editor-variable)" },
  { tag: [tags.special(tags.variableName), tags.local(tags.variableName)], color: "var(--editor-variable2)" },
  { tag: [tags.definition(tags.variableName), tags.function(tags.variableName), tags.function(tags.propertyName)], color: "var(--editor-type)" },
  { tag: [tags.typeName, tags.namespace, tags.macroName], color: "var(--editor-type)" },
  { tag: [tags.className, tags.definition(tags.typeName)], color: "var(--editor-class)" },
  { tag: [tags.propertyName, tags.attributeName, tags.definition(tags.propertyName)], color: "var(--editor-property)" },
  { tag: [tags.operator, tags.operatorKeyword, tags.compareOperator, tags.logicOperator, tags.arithmeticOperator, tags.definitionOperator], color: "var(--editor-operator)" },
  { tag: [tags.punctuation, tags.bracket, tags.separator], color: "var(--editor-punctuation)" },
  { tag: [tags.meta, tags.documentMeta, tags.annotation, tags.processingInstruction], color: "var(--editor-meta)" },
  { tag: [tags.link, tags.url], color: "var(--editor-link)", textDecoration: "underline" },
  { tag: [tags.heading, tags.heading1, tags.heading2, tags.heading3, tags.heading4, tags.heading5, tags.heading6], color: "var(--editor-heading)", fontWeight: "700" },
  { tag: tags.strong, fontWeight: "700" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: tags.inserted, color: "var(--editor-inserted)" },
  { tag: tags.deleted, color: "var(--editor-deleted)" },
  { tag: tags.invalid, color: "var(--editor-invalid)", textDecoration: "underline wavy var(--editor-invalid)" },
]);

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
