import { createPortal } from "react-dom";
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

type ContextMenuState = {
  x: number;
  y: number;
};

export function useContextMenu() {
  const [menu, setMenu] = useState<ContextMenuState | null>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("pointerdown", close);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", close);
    };
  }, [menu]);

  return {
    menu,
    openMenu: (event: React.MouseEvent) => {
      event.preventDefault();
      event.stopPropagation();
      setMenu({
        x: event.clientX,
        y: event.clientY,
      });
    },
    closeMenu: () => setMenu(null),
  };
}

export function ContextMenu({
  ariaLabel,
  menu,
  onClose,
  children,
}: {
  ariaLabel: string;
  menu: ContextMenuState | null;
  onClose: () => void;
  children: ReactNode;
}) {
  if (!menu) return null;
  const viewportWidth = typeof window === "undefined" ? 1024 : window.innerWidth;
  const viewportHeight = typeof window === "undefined" ? 768 : window.innerHeight;
  const content = (
    <>
      <div
        className="fixed inset-0 z-[9999]"
        aria-hidden="true"
        onContextMenu={(event) => {
          event.preventDefault();
          onClose();
        }}
        onPointerDown={(event) => {
          event.preventDefault();
          onClose();
        }}
        onTouchMove={(event) => event.preventDefault()}
        onWheel={(event) => event.preventDefault()}
      />
      <div
        role="menu"
        aria-label={ariaLabel}
        className="fixed z-[10000] min-w-[184px] rounded-[10px] border border-line-strong bg-surface-popover p-1 text-ink shadow-pop"
        style={{
          left: menu.x,
          top: menu.y,
          maxWidth: "min(260px, calc(100vw - 16px))",
          transform: `translate(${menu.x > viewportWidth - 260 ? "-100%" : "0"}, ${menu.y > viewportHeight - 220 ? "-100%" : "0"})`,
        }}
        onContextMenu={(event) => event.preventDefault()}
        onPointerDown={(event) => event.stopPropagation()}
        onTouchMove={(event) => event.stopPropagation()}
        onWheel={(event) => event.stopPropagation()}
      >
        <ContextMenuContext.Provider value={{ close: onClose }}>{children}</ContextMenuContext.Provider>
      </div>
    </>
  );
  return createPortal(content, document.body);
}

type ContextMenuContextValue = {
  close: () => void;
};

const ContextMenuContext = createContext<ContextMenuContextValue | null>(null);

export function ContextMenuItem({
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
  const context = useContext(ContextMenuContext);
  return (
    <button
      role="menuitem"
      type="button"
      disabled={disabled}
      tabIndex={-1}
      className="flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[12.5px] font-medium text-ink-soft outline-none transition-colors hover:bg-fill/8 hover:text-ink disabled:opacity-50"
      aria-label={label}
      onClick={() => {
        if (disabled) return;
        onSelect?.();
        context?.close();
      }}
    >
      {children}
    </button>
  );
}

export function ContextMenuSeparator() {
  return <div role="separator" className="my-1 h-px bg-line/70" />;
}
