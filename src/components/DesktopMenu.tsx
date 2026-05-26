import {
  autoUpdate,
  flip,
  FloatingPortal,
  offset,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useInteractions,
  useMergeRefs,
  useRole,
  type Placement as FloatingPlacement,
} from "@floating-ui/react";
import { DismissButton, Overlay, mergeProps, useOverlay, useOverlayPosition, usePress } from "react-aria";
import {
  cloneElement,
  createContext,
  useContext,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
  type Ref,
} from "react";

type DesktopMenuContextValue = {
  close: () => void;
};

const DesktopMenuContext = createContext<DesktopMenuContextValue | null>(null);

type DesktopMenuProps = {
  ariaLabel: string;
  children: ReactNode;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  placement?: FloatingPlacement;
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
  const { context, floatingStyles, refs } = useFloating({
    open: isOpen,
    onOpenChange,
    placement,
    transform: false,
    middleware: [offset(6), flip({ padding: 12 }), shift({ padding: 12 })],
    whileElementsMounted: autoUpdate,
  });
  const click = useClick(context, {
    event: "mousedown",
  });
  const dismiss = useDismiss(context, {
    outsidePress: true,
    outsidePressEvent: "pointerdown",
    escapeKey: true,
  });
  const role = useRole(context, { role: "menu" });
  const { getFloatingProps, getReferenceProps } = useInteractions([click, dismiss, role]);
  const triggerRef = (trigger as ReactElement & { ref?: Ref<Element> }).ref;
  const referenceRef = useMergeRefs([refs.setReference, triggerRef]);
  const close = () => onOpenChange(false);
  const floatingProps = getFloatingProps({
    role: "menu",
    "aria-label": ariaLabel,
    className:
      "desktop-menu-popover no-drag z-[10000] grid w-[min(240px,calc(100vw_-_24px))] gap-0.5 rounded-[10px] border border-border-subtle bg-surface-popover p-1 text-ink shadow-floating outline-none",
  });

  return (
    <DesktopMenuContext.Provider value={{ close }}>
      {renderMenuTrigger(
        trigger,
        ariaLabel,
        referenceRef,
        getReferenceProps({ className: "no-drag" }),
        isOpen,
      )}
      {isOpen && (
        <FloatingPortal preserveTabOrder={false}>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            {...(floatingProps as Record<string, unknown>)}
          >
            {children}
          </div>
        </FloatingPortal>
      )}
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
  const select = () => {
    if (disabled) return;
    onSelect?.();
    context.close();
  };

  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      aria-label={label}
      className="flex min-h-7 w-full min-w-0 items-center gap-2 overflow-hidden rounded-md px-2 py-1.5 text-left text-[12.5px] font-medium leading-4 text-ink-soft outline-none transition-colors hover:bg-default-hover hover:text-ink disabled:opacity-50 active:bg-default-hover active:text-ink"
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.stopPropagation();
        select();
      }}
      onClick={(event) => {
        if (event.detail === 0) select();
      }}
    >
      <span className="min-w-0 flex-1 truncate">{children}</span>
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
  const [isOpen, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const overlayRef = useRef<HTMLDivElement | null>(null);
  const { pressProps, isPressed } = usePress({
    ref: triggerRef,
    isDisabled: disabled,
    onPress: () => setOpen((value) => !value),
  });
  const { overlayProps } = useOverlay(
    {
      isOpen,
      isDismissable: true,
      onClose: () => setOpen(false),
      shouldCloseOnInteractOutside: (element) => element !== triggerRef.current && !triggerRef.current?.contains(element),
    },
    overlayRef,
  );
  const { overlayProps: positionProps } = useOverlayPosition({
    targetRef: triggerRef,
    overlayRef,
    isOpen,
    offset: 6,
    containerPadding: 12,
    shouldFlip: true,
    placement: "right top",
    onClose: () => setOpen(false),
  });

  return (
    <>
      <button
        {...pressProps}
        ref={triggerRef}
        type="button"
        role="menuitem"
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={isOpen}
        data-active-item={isPressed || isOpen ? "" : undefined}
        className="flex min-h-7 w-full min-w-0 items-center justify-between gap-2 overflow-hidden rounded-md px-2 py-1.5 text-left text-[12.5px] font-medium leading-4 text-ink-soft outline-none transition-colors hover:bg-default-hover hover:text-ink aria-disabled:opacity-50 data-[active-item]:bg-default-hover data-[active-item]:text-ink"
        onMouseEnter={() => {
          if (!disabled) setOpen(true);
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowRight") {
            event.preventDefault();
            setOpen(true);
          }
          if (event.key === "Escape") {
            setOpen(false);
          }
        }}
      >
        <span className="min-w-0 truncate">{label}</span>
        <span className="text-ink-faint">›</span>
      </button>
      {isOpen && (
        <Overlay disableFocusManagement>
          <div
            {...(mergeProps(overlayProps, positionProps) as Record<string, unknown>)}
            ref={overlayRef}
            role="menu"
            aria-label={label}
            className="desktop-menu-popover no-drag z-[10001] grid w-[min(240px,calc(100vw_-_24px))] gap-0.5 rounded-[10px] border border-border-subtle bg-surface-popover p-1 text-ink shadow-floating outline-none"
            onMouseLeave={() => setOpen(false)}
          >
            <DismissButton onDismiss={() => setOpen(false)} />
            {children}
            <DismissButton onDismiss={() => setOpen(false)} />
          </div>
        </Overlay>
      )}
    </>
  );
}

export function DesktopMenuSectionLabel({ children }: { children: ReactNode }) {
  return (
    <div role="presentation" className="min-w-0 truncate px-2 py-1 text-[11px] font-semibold text-ink-faint">
      {children}
    </div>
  );
}

export function DesktopMenuSeparator() {
  return <div role="separator" className="my-1 h-px bg-border-subtle/70" />;
}

function renderMenuTrigger(
  trigger: ReactElement<Record<string, unknown>>,
  ariaLabel: string,
  triggerRef: Ref<Element>,
  referenceProps: Parameters<typeof mergeProps>[number],
  isOpen: boolean,
) {
  const props = trigger.props;
  const label = typeof props["aria-label"] === "string" ? props["aria-label"] : ariaLabel;
  const className = typeof props.className === "string" ? props.className : undefined;
  return cloneElement(trigger, {
    ...mergeProps(props, referenceProps),
    ref: triggerRef,
    type: typeof props.type === "string" ? props.type : "button",
    "aria-label": label,
    "aria-haspopup": "menu",
    "aria-expanded": isOpen,
    className: `${className ?? ""} no-drag`,
  } as Record<string, unknown>);
}
