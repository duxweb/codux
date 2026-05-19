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
import { EditorState, RangeSetBuilder } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { basicSetup } from "codemirror";
import { openSearchPanel } from "@codemirror/search";
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef } from "react";
import { tm } from "../i18n";

export type CodeEditorScrollInfo = {
  ratio: number;
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
};

export type CodeEditorLineHighlight = {
  line: number;
  tone: "add" | "delete";
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
  silentScrollTop?: number;
  lineHighlights?: CodeEditorLineHighlight[];
};

export const CodeEditor = forwardRef<CodeEditorHandle, Props>(function CodeEditor(
  { value, documentKey, language, readOnly = false, onChange, onScrollInfoChange, initialScrollTop = 0, silentScrollTop, lineHighlights = [] },
  ref,
) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onScrollInfoChangeRef = useRef(onScrollInfoChange);
  const suppressNextScrollInfoRef = useRef(false);
  const languageExtension = useMemo(() => extensionForLanguage(language), [language]);
  const lineHighlightExtension = useMemo(() => extensionForLineHighlights(lineHighlights), [lineHighlights]);
  const phrases = useMemo(() => codeMirrorPhrases(), []);

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
      if (suppressNextScrollInfoRef.current) {
        suppressNextScrollInfoRef.current = false;
        return;
      }
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
      lineHighlightExtension,
      EditorState.phrases.of(phrases),
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
  }, [documentKey, languageExtension, lineHighlightExtension, phrases, readOnly]);

  useEffect(() => {
    if (silentScrollTop === undefined) return;
    const scrollDOM = viewRef.current?.scrollDOM;
    if (!scrollDOM) return;
    const next = Math.max(0, silentScrollTop);
    if (Math.abs(scrollDOM.scrollTop - next) < 1) return;
    suppressNextScrollInfoRef.current = true;
    scrollDOM.scrollTop = next;
  }, [silentScrollTop]);

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

function extensionForLineHighlights(highlights: CodeEditorLineHighlight[]): Extension {
  if (highlights.length === 0) return [];
  const sorted = [...highlights]
    .filter((highlight) => Number.isFinite(highlight.line) && highlight.line > 0)
    .sort((left, right) => left.line - right.line);
  return EditorView.decorations.compute(["doc"], (state) => {
    const builder = new RangeSetBuilder<Decoration>();
    let previousLine = 0;
    for (const highlight of sorted) {
      const line = Math.floor(highlight.line);
      if (line === previousLine || line > state.doc.lines) continue;
      previousLine = line;
      const from = state.doc.line(line).from;
      builder.add(
        from,
        from,
        Decoration.line({ class: highlight.tone === "add" ? "cm-line-diff-add" : "cm-line-diff-delete" }),
      );
    }
    return builder.finish();
  });
}

function codeMirrorPhrases() {
  return {
    Find: tm("files.preview.search.find", "Find"),
    Replace: tm("files.preview.search.replace", "Replace"),
    next: tm("files.preview.search.next", "Next"),
    previous: tm("files.preview.search.previous", "Previous"),
    all: tm("files.preview.search.all", "All"),
    "match case": tm("files.preview.search.match_case", "Match case"),
    regexp: tm("files.preview.search.regexp", "Regex"),
    "by word": tm("files.preview.search.by_word", "Whole word"),
    replace: tm("files.preview.search.replace_action", "Replace"),
    "replace all": tm("files.preview.search.replace_all", "Replace all"),
    close: tm("files.preview.search.close", "Close"),
    "current match": tm("files.preview.search.current_match", "Current match"),
    "on line": tm("files.preview.search.on_line", "on line"),
    "replaced match on line $": tm("files.preview.search.replaced_match_on_line_format", "Replaced match on line $"),
    "replaced $ matches": tm("files.preview.search.replaced_matches_format", "Replaced $ matches"),
    "Go to line": tm("files.preview.search.go_to_line", "Go to line"),
    go: tm("files.preview.search.go", "Go"),
    "Selection deleted": tm("files.preview.selection_deleted", "Selection deleted"),
    "Folded lines": tm("files.preview.folded_lines", "Folded lines"),
    "Unfolded lines": tm("files.preview.unfolded_lines", "Unfolded lines"),
    to: tm("files.preview.to", "to"),
    "folded code": tm("files.preview.folded_code", "folded code"),
    unfold: tm("files.preview.unfold", "unfold"),
    "Fold line": tm("files.preview.fold_line", "Fold line"),
    "Unfold line": tm("files.preview.unfold_line", "Unfold line"),
    "Control character": tm("files.preview.control_character", "Control character"),
  };
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
    ".cm-panels": {
      zIndex: "30",
    },
    ".cm-panels-top": {
      position: "absolute !important",
      top: "10px",
      left: "14px",
      right: "98px",
      borderBottom: "0 !important",
      backgroundColor: "transparent !important",
      pointerEvents: "none",
    },
    ".cm-content": {
      padding: "14px 0 24px",
      caretColor: "var(--terminal-cursor)",
      minHeight: "100%",
    },
    ".cm-line": {
      padding: "0 18px 0 8px",
    },
    ".cm-line-diff-add": {
      backgroundColor: "color-mix(in oklab, var(--color-brand-green) 13%, transparent)",
      boxShadow: "inset 2px 0 0 color-mix(in oklab, var(--color-brand-green) 72%, transparent)",
    },
    ".cm-line-diff-delete": {
      backgroundColor: "color-mix(in oklab, var(--color-brand-red) 13%, transparent)",
      boxShadow: "inset 2px 0 0 color-mix(in oklab, var(--color-brand-red) 72%, transparent)",
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
    ".cm-panel.cm-search": {
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "6px",
      width: "min(680px, 100%)",
      minHeight: "42px",
      marginLeft: "auto",
      padding: "7px 34px 7px 8px",
      pointerEvents: "auto",
      border: "1px solid var(--color-line-strong)",
      borderRadius: "10px",
      backgroundColor: "color-mix(in oklab, var(--color-surface-chrome) 94%, transparent)",
      boxShadow: "var(--shadow-pop)",
      backdropFilter: "blur(18px)",
      WebkitBackdropFilter: "blur(18px)",
      color: "var(--color-ink)",
    },
    ".cm-panel.cm-search br": {
      display: "none",
    },
    ".cm-panel.cm-search input.cm-textfield": {
      height: "28px",
      minWidth: "0",
      border: "1px solid var(--color-line)",
      borderRadius: "7px",
      backgroundColor: "color-mix(in oklab, var(--color-fill) 5%, transparent)",
      padding: "0 9px",
      color: "var(--color-ink)",
      outline: "none",
      font: "inherit",
      fontSize: "12.5px",
    },
    ".cm-panel.cm-search input.cm-textfield:focus": {
      borderColor: "color-mix(in oklab, var(--color-brand-blue) 72%, var(--color-line))",
      boxShadow: "0 0 0 2px color-mix(in oklab, var(--color-brand-blue) 18%, transparent)",
    },
    ".cm-panel.cm-search input[name=search]": {
      flex: "1 1 220px",
    },
    ".cm-panel.cm-search input[name=replace]": {
      flex: "1 1 190px",
    },
    ".cm-panel.cm-search button": {
      display: "inline-flex",
      alignItems: "center",
      justifyContent: "center",
      height: "28px",
      minWidth: "28px",
      border: "1px solid var(--color-line)",
      borderRadius: "7px",
      backgroundColor: "color-mix(in oklab, var(--color-fill) 5%, transparent)",
      padding: "0 8px",
      color: "var(--color-ink-soft)",
      font: "inherit",
      fontSize: "12px",
      fontWeight: "600",
    },
    ".cm-panel.cm-search button:hover": {
      borderColor: "var(--color-line-strong)",
      backgroundColor: "color-mix(in oklab, var(--color-fill) 10%, transparent)",
      color: "var(--color-ink)",
    },
    ".cm-panel.cm-search button[name=next], .cm-panel.cm-search button[name=prev], .cm-panel.cm-search button[name=select]": {
      width: "30px",
      padding: "0",
      overflow: "hidden",
      textIndent: "-999px",
      position: "relative",
    },
    ".cm-panel.cm-search button[name=next]::after, .cm-panel.cm-search button[name=prev]::after, .cm-panel.cm-search button[name=select]::after": {
      position: "absolute",
      inset: "0",
      display: "grid",
      placeItems: "center",
      textIndent: "0",
      fontSize: "14px",
      lineHeight: "1",
    },
    ".cm-panel.cm-search button[name=next]::after": {
      content: '"↓"',
    },
    ".cm-panel.cm-search button[name=prev]::after": {
      content: '"↑"',
    },
    ".cm-panel.cm-search button[name=select]::after": {
      content: '"∗"',
    },
    ".cm-panel.cm-search label": {
      display: "inline-flex",
      alignItems: "center",
      gap: "5px",
      height: "28px",
      border: "1px solid var(--color-line)",
      borderRadius: "999px",
      backgroundColor: "color-mix(in oklab, var(--color-fill) 4%, transparent)",
      padding: "0 9px 0 7px",
      color: "var(--color-ink-mute)",
      fontSize: "11.5px",
      fontWeight: "600",
      whiteSpace: "nowrap",
    },
    ".cm-panel.cm-search label:hover": {
      color: "var(--color-ink-soft)",
      backgroundColor: "color-mix(in oklab, var(--color-fill) 7%, transparent)",
    },
    ".cm-panel.cm-search input[type=checkbox]": {
      width: "13px",
      height: "13px",
      margin: "0",
      accentColor: "var(--color-brand-blue)",
    },
    ".cm-panel.cm-search [name=close]": {
      position: "absolute",
      top: "7px",
      right: "7px",
      width: "28px",
      padding: "0",
      borderColor: "transparent",
      backgroundColor: "transparent",
      color: "var(--color-ink-faint)",
      fontSize: "18px",
      fontWeight: "500",
    },
    ".cm-panel.cm-search [name=close]:hover": {
      backgroundColor: "color-mix(in oklab, var(--color-fill) 8%, transparent)",
      color: "var(--color-ink)",
    },
    ".cm-searchMatch": {
      backgroundColor: "color-mix(in oklab, var(--color-brand-amber) 42%, transparent)",
      outline: "1px solid color-mix(in oklab, var(--color-brand-amber) 56%, transparent)",
    },
    ".cm-searchMatch-selected": {
      backgroundColor: "color-mix(in oklab, var(--color-brand-blue) 38%, transparent)",
      outline: "1px solid color-mix(in oklab, var(--color-brand-blue) 70%, transparent)",
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
