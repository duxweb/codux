import {
  autoUpdate,
  flip,
  FloatingPortal,
  offset,
  shift,
  useFloating,
  useHover,
  useInteractions,
  useRole,
  useTransitionStyles,
  type Placement,
} from "@floating-ui/react";
import { cloneElement, type ReactElement, type ReactNode, type Ref } from "react";

export type TooltipPlacement = "top" | "bottom" | "left" | "right";

type Props = {
  label: ReactNode;
  placement?: TooltipPlacement;
  delay?: number;
  disabled?: boolean;
  triggerClassName?: string;
  contentClassName?: string;
  children: ReactElement;
};

type TriggerProps = Record<string, unknown> & {
  className?: string;
  ref?: Ref<Element>;
};

export function Tooltip({
  label,
  placement = "bottom",
  delay = 300,
  disabled,
  triggerClassName = "inline-block max-w-full align-middle",
  contentClassName,
  children,
}: Props) {
  const isDisabled = disabled || !label;
  const { context, floatingStyles, refs } = useFloating({
    open: undefined,
    placement: placement as Placement,
    middleware: [offset(6), flip({ padding: 8 }), shift({ padding: 8 })],
    whileElementsMounted: autoUpdate,
  });
  const hover = useHover(context, {
    enabled: !isDisabled,
    delay: { open: delay, close: 80 },
    mouseOnly: true,
  });
  const role = useRole(context, { role: "tooltip" });
  const { getReferenceProps, getFloatingProps } = useInteractions([hover, role]);
  const { isMounted, styles: transitionStyles } = useTransitionStyles(context, {
    duration: 90,
    initial: { opacity: 0, transform: "translateY(-1px) scale(0.985)" },
    open: { opacity: 1, transform: "translateY(0) scale(1)" },
    close: { opacity: 0, transform: "translateY(-1px) scale(0.985)" },
  });

  if (isDisabled) {
    return children;
  }

  const triggerRef = (children as ReactElement & { ref?: Ref<Element> }).ref;
  const originalProps = children.props as Record<string, unknown>;
  const referenceProps = getReferenceProps({
    ref: mergeRefs(refs.setReference, triggerRef),
    className: mergeClassName(
      typeof originalProps.className === "string" ? originalProps.className : undefined,
      triggerClassName,
      "no-drag",
    ),
  }) as TriggerProps;

  return (
    <>
      {cloneElement(children, referenceProps)}
      {isMounted && (
        <FloatingPortal preserveTabOrder={false}>
          <div
            ref={refs.setFloating}
            style={{ ...floatingStyles, ...transitionStyles }}
            className={`z-[10050] max-w-[260px] rounded-md border border-line-strong bg-surface-chrome px-2 py-1 text-[11.5px] font-medium text-ink-soft shadow-pop no-drag ${contentClassName ?? ""}`}
            {...getFloatingProps()}
          >
            {label}
          </div>
        </FloatingPortal>
      )}
    </>
  );
}

function mergeClassName(...items: Array<string | undefined>) {
  return items.filter(Boolean).join(" ");
}

function mergeRefs<T>(...refs: Array<Ref<T> | undefined>) {
  return (node: T | null) => {
    for (const ref of refs) {
      if (typeof ref === "function") {
        ref(node);
      } else if (ref) {
        ref.current = node;
      }
    }
  };
}
