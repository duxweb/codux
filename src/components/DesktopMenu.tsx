import { createContext, useContext, useState, type ReactElement, type ReactNode } from "react";
import { DesktopPopover } from "./DesktopPopover";
import type { Placement } from "@floating-ui/react";

type DesktopMenuContextValue = {
  close: () => void;
};

const DesktopMenuContext = createContext<DesktopMenuContextValue | null>(null);

type DesktopMenuProps = {
  ariaLabel: string;
  children: ReactNode;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  placement?: Placement;
  trigger: ReactElement<Record<string, unknown>>;
};

export function DesktopMenu({
  ariaLabel,
  children,
  isOpen,
  onOpenChange,
  placement = "bottom-end",
  trigger,
}: DesktopMenuProps) {
  return (
    <DesktopMenuContext.Provider
      value={{
        close: () => onOpenChange(false),
      }}
    >
      <DesktopPopover
        isOpen={isOpen}
        onOpenChange={onOpenChange}
        placement={placement}
        role="menu"
        trigger={trigger}
        contentClassName="min-w-[184px] p-1"
      >
        <div role="menu" aria-label={ariaLabel}>
          {children}
        </div>
      </DesktopPopover>
    </DesktopMenuContext.Provider>
  );
}

export function DesktopMenuItem({
  children,
  disabled,
  label,
  onSelect,
}: {
  children: ReactNode;
  disabled?: boolean;
  label: string;
  onSelect?: () => void;
}) {
  const context = useContext(DesktopMenuContext);
  if (!context) {
    throw new Error("DesktopMenuItem must be used inside DesktopMenu");
  }
  return (
    <button
      role="menuitem"
      type="button"
      disabled={disabled}
      tabIndex={-1}
      className="flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[12.5px] font-medium text-ink-soft outline-none transition-colors hover:bg-fill/8 hover:text-ink disabled:opacity-50"
      onClick={() => {
        if (disabled) return;
        onSelect?.();
        context.close();
      }}
    >
      {children}
    </button>
  );
}

export function DesktopSubmenu({
  children,
  disabled,
  label,
}: {
  children: ReactNode;
  disabled?: boolean;
  label: string;
}) {
  const context = useContext(DesktopMenuContext);
  const [isOpen, setOpen] = useState(false);
  if (!context) {
    throw new Error("DesktopSubmenu must be used inside DesktopMenu");
  }
  return (
    <DesktopMenuContext.Provider value={context}>
      <DesktopPopover
        isOpen={isOpen}
        onOpenChange={setOpen}
        placement="right-start"
        role="menu"
        hover
        hoverDelay={{ open: 80, close: 120 }}
        offsetValue={4}
        trigger={
          <button
            role="menuitem"
            type="button"
            disabled={disabled}
            tabIndex={-1}
            className="flex h-7 w-full items-center justify-between gap-3 rounded-md px-2 text-left text-[12.5px] font-medium text-ink-soft outline-none transition-colors hover:bg-fill/8 hover:text-ink disabled:opacity-50"
          >
            <span className="min-w-0 truncate">{label}</span>
            <span className="text-ink-faint">&gt;</span>
          </button>
        }
        contentClassName="min-w-[184px] p-1"
      >
        <div role="menu" aria-label={label}>
          {children}
        </div>
      </DesktopPopover>
    </DesktopMenuContext.Provider>
  );
}

export function DesktopMenuSectionLabel({ children }: { children: ReactNode }) {
  return <div className="px-2 py-1 text-[11px] font-semibold text-ink-faint">{children}</div>;
}

export function DesktopMenuSeparator() {
  return <div role="separator" className="my-1 h-px bg-line/70" />;
}
