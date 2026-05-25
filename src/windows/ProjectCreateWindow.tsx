import {
  Book,
  Box,
  Boxes,
  FileText,
  Folder,
  Globe,
  Hammer,
  Laptop,
  Package,
  Palette,
  Server,
  Sparkles,
  TerminalSquare,
  Users,
  Wrench,
  Zap,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import { Button as HeroButton } from "@heroui/react";
import { SettingsForm, TextInput } from "../components/Form";
import { PressableButton } from "../components/PressableButton";
import { tm } from "../i18n";
import { openLocalizedDialog } from "../localizedDialog";
import { displayPath } from "../pathDisplay";
import { closeCurrentAppWindow, revealCurrentAppWindow } from "../windowing";
import { WindowFooterActions, WindowFrame } from "./WindowFrame";

type SymbolItem = {
  id: string;
  icon?: typeof Folder;
};

const SYMBOLS: SymbolItem[] = [
  { id: "none" },
  { id: "terminal", icon: TerminalSquare },
  { id: "folder", icon: Folder },
  { id: "shippingbox", icon: Box },
  { id: "hammer", icon: Hammer },
  { id: "server.rack", icon: Server },
  { id: "globe", icon: Globe },
  { id: "bolt", icon: Zap },
  { id: "wrench", icon: Wrench },
  { id: "doc.text", icon: FileText },
  { id: "shippingbox.fill", icon: Package },
  { id: "laptopcomputer", icon: Laptop },
  { id: "cube.box", icon: Boxes },
  { id: "paintpalette", icon: Palette },
  { id: "sparkles", icon: Sparkles },
  { id: "book", icon: Book },
  { id: "person.2", icon: Users },
];

const COLORS = [
  "#0A84FF",
  "#8C52FF",
  "#4C8BF5",
  "#15B8A6",
  "#32C766",
  "#FFB020",
  "#FF7A59",
  "#FF5C8A",
  "#7B61FF",
  "#00A3FF",
  "#6D9F71",
];

export function ProjectCreateWindow() {
  const query = new URLSearchParams(window.location.hash.split("?")[1] ?? "");
  const projectId = query.get("projectId") ?? "";
  const isEditing = projectId.trim().length > 0;
  const initialName = query.get("name") ?? "";
  const initialPath = query.get("path") ?? "";
  const initialSymbol = query.get("badgeSymbol") || "none";
  const initialColor = query.get("badgeColorHex") || COLORS[0];
  const [name, setName] = useState(initialName);
  const [path, setPath] = useState(initialPath);
  const [isPathFocused, setPathFocused] = useState(false);
  const [symbolId, setSymbolId] = useState<string>(initialSymbol);
  const [color, setColor] = useState<string>(initialColor);
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setSubmitting] = useState(false);

  const canSubmit = name.trim().length > 0 && path.trim().length > 0 && !isSubmitting;

  useEffect(() => {
    void revealCurrentAppWindow();
  }, []);

  const dismiss = () => {
    void closeCurrentAppWindow();
  };

  const chooseDirectory = async () => {
    try {
      if (!window.__TAURI_INTERNALS__) return;
      const selected = await openLocalizedDialog({
        directory: true,
        multiple: false,
        title: tm("project.editor.choose_directory.title", "Choose Project Directory"),
        message: tm("project.editor.choose_directory.message", "Select a folder for this project."),
        prompt: tm("project.editor.choose_directory.prompt", "Choose"),
      });
      if (typeof selected === "string") {
        setPath(selected);
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const submit = async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      if (window.__TAURI_INTERNALS__) {
        await invoke(isEditing ? "project_update" : "project_create", {
          request: {
            projectId: isEditing ? projectId : undefined,
            name: name.trim(),
            path: path.trim(),
            badgeText: name.trim().slice(0, 2).toUpperCase(),
            badgeSymbol: symbolId === "none" ? null : symbolId,
            badgeColorHex: color,
          },
        });
      }
      await closeCurrentAppWindow();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <WindowFrame
      title={isEditing ? tm("project.edit.title", "Edit Project") : tm("project.create.title", "Create Project")}
      footer={
        <WindowFooterActions
          onCancel={dismiss}
          onSubmit={() => void submit()}
          submitLabel={isEditing ? tm("common.save", "Save") : tm("common.create", "Create")}
          disabled={!canSubmit}
          busy={isSubmitting}
        />
      }
    >
      <SettingsForm className="gap-5">
        <FieldGroup label={tm("project.editor.name", "Project Name")} required>
          <TextInput
            placeholder={tm("common.required", "Required")}
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
          />
        </FieldGroup>

        <FieldGroup label={tm("project.editor.directory", "Project Directory")} required>
          <div className="flex gap-2">
            <TextInput
              placeholder="/path/to/project"
              value={isPathFocused ? path : displayPath(path)}
              onFocus={() => setPathFocused(true)}
              onBlur={() => setPathFocused(false)}
              onChange={(event) => setPath(event.currentTarget.value)}
              className="min-w-0 flex-1 font-mono"
            />
            <HeroButton
              size="md"
              variant="secondary"
              className="h-10 min-w-0 px-4"
              onPress={() => void chooseDirectory()}
            >
              {tm("common.choose", "Choose")}
            </HeroButton>
          </div>
        </FieldGroup>

        <FieldGroup label={tm("project.editor.icon", "Project Icon")}>
          <div className="grid grid-cols-10 gap-2">
            {SYMBOLS.map((sym) => (
              <SymbolCell
                key={sym.id}
                symbol={sym}
                selected={symbolId === sym.id}
                accent={color}
                onPress={() => setSymbolId(sym.id)}
              />
            ))}
          </div>
        </FieldGroup>

        <FieldGroup label={tm("project.editor.color", "Project Color")}>
          <div className="flex flex-wrap items-center gap-3 pt-1 pl-[3px]">
            {COLORS.map((value) => (
              <ColorDot key={value} color={value} selected={color === value} onPress={() => setColor(value)} />
            ))}
          </div>
        </FieldGroup>

        {error && (
          <div className="rounded-[8px] border border-brand-red/35 bg-brand-red/10 px-3 py-2 text-sm text-brand-red">
            {error}
          </div>
        )}
      </SettingsForm>
    </WindowFrame>
  );
}

function FieldGroup({ label, required, children }: { label: ReactNode; required?: boolean; children: ReactNode }) {
  return (
    <div className="grid gap-2">
      <div className="text-sm font-medium text-ink">
        {label}
        {required && <span className="text-brand-red ml-0.5">*</span>}
      </div>
      {children}
    </div>
  );
}

function SymbolCell({
  symbol,
  selected,
  accent,
  onPress,
}: {
  symbol: SymbolItem;
  selected: boolean;
  accent: string;
  onPress: () => void;
}) {
  return (
    <PressableButton
      onPressUp={onPress}
      aria-pressed={selected}
      aria-label={symbol.id === "none" ? tm("common.none", "None") : symbol.id}
      className={`relative aspect-square rounded-[8px] grid place-items-center transition-colors ${
        selected
          ? "bg-fill/[0.12] border border-border opacity-100"
          : "bg-fill/[0.05] border border-border-subtle opacity-60 hover:opacity-85 hover:bg-fill/[0.08] hover:border-border"
      }`}
    >
      {symbol.icon ? (
        <symbol.icon size={17} strokeWidth={1.8} style={{ color: accent }} />
      ) : (
        <span className="text-xs font-semibold text-ink">{tm("common.none", "None")}</span>
      )}
    </PressableButton>
  );
}

function ColorDot({ color, selected, onPress }: { color: string; selected: boolean; onPress: () => void }) {
  return (
    <PressableButton
      onPressUp={onPress}
      className={`relative w-7 h-7 rounded-full transition-[opacity,transform] hover:scale-[1.08] ${
        selected ? "opacity-100" : "opacity-65 hover:opacity-90"
      }`}
      style={{
        background: color,
        boxShadow: selected
          ? "0 0 0 2px rgb(255 255 255 / 0.96), 0 0 0 4px rgb(0 0 0 / 0.42), 0 2px 5px rgb(0 0 0 / 0.28)"
          : "0 1px 2px rgb(0 0 0 / 0.25)",
      }}
      aria-label={color}
      aria-pressed={selected}
    />
  );
}
