import type { ReactNode } from "react";
import { Button } from "./Button";
import { PressableButton } from "./PressableButton";
import { Tooltip } from "./Tooltip";
import type { AppIcon } from "../icons";
import { Plus, X } from "../icons";
import { tm } from "../i18n";

export type TabStripItem = {
  id: string;
  label: ReactNode;
  icon?: AppIcon;
  closable?: boolean;
};

type Props = {
  items: TabStripItem[];
  activeId: string;
  addLabel?: string;
  className?: string;
  emptyLabel?: ReactNode;
  onSelect: (id: string) => void;
  onClose?: (id: string) => void;
  onAdd?: () => void;
};

export function TabStrip({
  items,
  activeId,
  addLabel = tm("workspace.create_tab", "New Tab"),
  className,
  emptyLabel,
  onSelect,
  onClose,
  onAdd,
}: Props) {
  return (
    <div className={`tab-strip ${className ?? ""}`}>
      <div className="tab-strip-scroll">
        {items.length === 0 && emptyLabel ? (
          <div className="tab-strip-empty">{emptyLabel}</div>
        ) : (
          items.map((item) => {
            const Icon = item.icon;
            const active = item.id === activeId;
            return (
              <div
                key={item.id}
                className={`tab-strip-item group ${active ? "active" : ""}`}
              >
                <PressableButton
                  className="tab-strip-select"
                  onPressUp={() => onSelect(item.id)}
                >
                  {Icon ? <Icon size={13} strokeWidth={2.1} className="flex-shrink-0" /> : null}
                  <span className="truncate">{item.label}</span>
                </PressableButton>
                {item.closable && onClose ? (
                  <PressableButton
                    aria-label={tm("terminal.tab.close", "Close Tab")}
                    className={`tab-strip-close ${active ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
                    onPressUp={(event) => {
                      event.continuePropagation();
                      onClose(item.id);
                    }}
                  >
                    <X size={11} strokeWidth={2.2} />
                  </PressableButton>
                ) : null}
              </div>
            );
          })
        )}
      </div>
      {onAdd ? (
        <Tooltip label={addLabel} placement="bottom">
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            onPress={onAdd}
            aria-label={addLabel}
            className="h-6 w-6 min-w-6 flex-shrink-0 text-ink-mute"
          >
            <Plus size={13} strokeWidth={2.2} />
          </Button>
        </Tooltip>
      ) : null}
    </div>
  );
}
