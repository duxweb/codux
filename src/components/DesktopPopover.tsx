import {
  autoUpdate,
  flip,
  FloatingPortal,
  offset,
  safePolygon,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useHover,
  useInteractions,
  useMergeRefs,
  useRole,
  useTransitionStyles,
  type Delay,
  type Placement,
} from "@floating-ui/react";
import { cloneElement, type CSSProperties, type ReactElement, type ReactNode, type Ref } from "react";

type TriggerProps = Record<string, unknown> & {
  className?: string;
  ref?: (node: Element | null) => void;
};

export type DesktopPopoverProps = {
  children: ReactNode;
  className?: string;
  contentClassName?: string;
  contentStyle?: CSSProperties;
  hover?: boolean;
  hoverDelay?: Delay;
  isOpen: boolean;
  matchReferenceWidth?: boolean;
  offsetValue?: number;
  onOpenChange: (isOpen: boolean) => void;
  placement?: Placement;
  role?: "dialog" | "menu" | "tooltip" | "listbox";
  trigger: ReactElement<Record<string, unknown>>;
};

export function DesktopPopover({
  children,
  className,
  contentClassName,
  contentStyle,
  hover,
  hoverDelay,
  isOpen,
  matchReferenceWidth,
  offsetValue = 8,
  onOpenChange,
  placement = "bottom-end",
  role = "dialog",
  trigger,
}: DesktopPopoverProps) {
  const isHover = hover === true;
  const { context, floatingStyles, refs } = useFloating({
    open: isOpen,
    onOpenChange,
    placement,
    transform: false,
    middleware: [offset(offsetValue), flip({ padding: 8 }), shift({ padding: 8 })],
    whileElementsMounted: autoUpdate,
  });
  const click = useClick(context, {
    enabled: !isHover,
    event: "mousedown",
  });
  const hoverInteraction = useHover(context, {
    enabled: isHover,
    delay: hoverDelay,
    handleClose: safePolygon(),
    mouseOnly: true,
  });
  const dismiss = useDismiss(context, {
    outsidePress: true,
    outsidePressEvent: "pointerdown",
    escapeKey: true,
  });
  const roleInteraction = useRole(context, { role });
  const { getFloatingProps, getReferenceProps } = useInteractions([click, hoverInteraction, dismiss, roleInteraction]);
  const { isMounted, styles: transitionStyles } = useTransitionStyles(context, {
    duration: 110,
    initial: { opacity: 0, transform: "scale(0.985)" },
    open: { opacity: 1, transform: "scale(1)" },
    close: { opacity: 0, transform: "scale(0.985)" },
  });

  const referenceProps = getReferenceProps({
    className: "no-drag",
  }) as TriggerProps;
  const triggerProps = trigger.props;
  const mergedReferenceProps = mergeElementProps(triggerProps, referenceProps);
  const triggerRef = (trigger as ReactElement & { ref?: Ref<Element> }).ref;
  const referenceRef = useMergeRefs([refs.setReference, triggerRef]);

  return (
    <>
      {cloneElement(trigger, {
        ...mergedReferenceProps,
        ref: referenceRef,
      } as Record<string, unknown>)}
      {isMounted && (
        <FloatingPortal preserveTabOrder={false}>
          <div
            ref={refs.setFloating}
            className={mergeClassName(
              "z-50 rounded-[10px] border border-line-strong bg-surface-chrome/95 text-ink shadow-pop outline-none backdrop-blur-xl no-drag",
              className,
            )}
            style={{
              ...floatingStyles,
              ...transitionStyles,
              width: matchReferenceWidth ? refs.reference.current?.getBoundingClientRect().width : undefined,
              transformOrigin: "var(--floating-transform-origin)",
              ...contentStyle,
            }}
            {...getFloatingProps()}
          >
            <div className={contentClassName}>{children}</div>
          </div>
        </FloatingPortal>
      )}
    </>
  );
}

function mergeClassName(...items: Array<string | undefined>) {
  return items.filter(Boolean).join(" ");
}

function mergeElementProps(original: Record<string, unknown>, injected: TriggerProps): TriggerProps {
  const props: TriggerProps = { ...injected };
  props.className = mergeClassName(
    typeof original.className === "string" ? original.className : undefined,
    injected.className,
  );
  props.onClick = mergeHandlers(original.onClick, injected.onClick);
  props.onMouseDown = mergeHandlers(original.onMouseDown, injected.onMouseDown);
  props.onPointerDown = mergeHandlers(original.onPointerDown, injected.onPointerDown);
  props.onKeyDown = mergeHandlers(original.onKeyDown, injected.onKeyDown);
  props.onKeyUp = mergeHandlers(original.onKeyUp, injected.onKeyUp);
  return props;
}

function mergeHandlers(first: unknown, second: unknown) {
  if (typeof first !== "function") return second;
  if (typeof second !== "function") return first;
  return (event: unknown) => {
    first(event);
    second(event);
  };
}
