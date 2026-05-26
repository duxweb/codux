import { createPortal } from "react-dom";
import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type HTMLAttributes,
  type ReactNode,
} from "react";

type DropdownContextValue = {
  close: () => void;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
};

type MenuContextValue = {
  onAction?: (key: string) => void;
  close: () => void;
};

const DropdownContext = createContext<DropdownContextValue | null>(null);
const MenuContext = createContext<MenuContextValue | null>(null);

type DropdownProps = {
  isOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
};

export function Dropdown({ isOpen, onOpenChange, children }: DropdownProps) {
  const [internalOpen, setInternalOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const open = isOpen ?? internalOpen;
  const setOpen = (next: boolean) => {
    onOpenChange?.(next);
    if (isOpen === undefined) setInternalOpen(next);
  };

  return (
    <DropdownContext.Provider
      value={{
        close: () => setOpen(false),
        isOpen: open,
        onOpenChange: setOpen,
        triggerRef,
      }}
    >
      {children}
    </DropdownContext.Provider>
  );
}

type TriggerProps = HTMLAttributes<HTMLButtonElement> & {
  isDisabled?: boolean;
};

function Trigger({ children, className, isDisabled, onClick, ...props }: TriggerProps) {
  const context = useDropdownContext();
  return (
    <button
      ref={context.triggerRef}
      type="button"
      disabled={isDisabled}
      className={`${className ?? ""} no-drag`}
      onPointerDown={(event) => {
        event.preventDefault();
        event.stopPropagation();
        context.onOpenChange(!context.isOpen);
      }}
      onClick={(event) => {
        onClick?.(event);
        if (!event.defaultPrevented && event.detail === 0) {
          context.onOpenChange(!context.isOpen);
        }
      }}
      {...props}
    >
      {children}
    </button>
  );
}

type PopoverProps = HTMLAttributes<HTMLDivElement> & {
  placement?: string;
  offset?: number;
};

function Popover({ children, className, placement = "bottom end", offset = 6, style, ...props }: PopoverProps) {
  const context = useDropdownContext();
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState<CSSProperties>({});

  useLayoutEffect(() => {
    if (!context.isOpen) return;
    const update = () => {
      const rect = context.triggerRef.current?.getBoundingClientRect();
      const popover = popoverRef.current;
      if (!rect) return;
      const width = popover?.offsetWidth ?? 210;
      const height = popover?.offsetHeight ?? 160;
      const alignEnd = placement.includes("end") || placement.includes("right");
      const left = alignEnd ? rect.right - width : rect.left;
      const top = placement.startsWith("top") ? rect.top - height - offset : rect.bottom + offset;
      setPosition({
        left: Math.max(8, Math.min(left, window.innerWidth - width - 8)),
        top: Math.max(8, Math.min(top, window.innerHeight - height - 8)),
      });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [context.isOpen, context.triggerRef, offset, placement]);

  useEffect(() => {
    if (!context.isOpen) return;
    const close = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && (context.triggerRef.current?.contains(target) || popoverRef.current?.contains(target))) return;
      context.close();
    };
    const closeOnKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") context.close();
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", closeOnKey);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", closeOnKey);
    };
  }, [context]);

  if (!context.isOpen) return null;

  return createPortal(
    <div
      ref={popoverRef}
      className={`fixed z-[10000] ${className ?? ""}`}
      style={{ ...position, ...style }}
      onContextMenu={(event) => event.preventDefault()}
      {...props}
    >
      {children}
    </div>,
    document.body,
  );
}

type MenuProps = HTMLAttributes<HTMLDivElement> & {
  onAction?: (key: string) => void;
  shouldCloseOnSelect?: boolean;
};

function Menu({ children, className, onAction, shouldCloseOnSelect: _shouldCloseOnSelect, ...props }: MenuProps) {
  const dropdown = useDropdownContext();
  return (
    <MenuContext.Provider value={{ onAction, close: dropdown.close }}>
      <div role="menu" className={className} {...props}>
        {children}
      </div>
    </MenuContext.Provider>
  );
}

type ItemProps = HTMLAttributes<HTMLButtonElement> & {
  id?: string;
  isDisabled?: boolean;
  textValue?: string;
};

function Item({ children, className, id, isDisabled, textValue: _textValue, onClick, ...props }: ItemProps) {
  const context = useContext(MenuContext);
  return (
    <button
      role="menuitem"
      type="button"
      disabled={isDisabled}
      tabIndex={-1}
      className={`flex min-h-7 w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12.5px] font-medium leading-4 text-ink-soft outline-none transition-colors hover:bg-default-hover hover:text-ink disabled:opacity-50 ${className ?? ""}`}
      onClick={(event) => {
        onClick?.(event);
        if (event.defaultPrevented || isDisabled) return;
        context?.onAction?.(String(id ?? ""));
        context?.close();
      }}
      {...props}
    >
      {children}
    </button>
  );
}

function useDropdownContext() {
  const context = useContext(DropdownContext);
  if (!context) throw new Error("Dropdown components must be used inside Dropdown");
  return context;
}

Dropdown.Trigger = Trigger;
Dropdown.Popover = Popover;
Dropdown.Menu = Menu;
Dropdown.Item = Item;
