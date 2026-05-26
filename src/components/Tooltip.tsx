import { useFloating, offset, flip, shift, useHover, useInteractions, FloatingPortal } from "@floating-ui/react";
import { cloneElement, useState, type ReactElement, type ReactNode } from "react";

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

export function Tooltip({
  label,
  placement = "bottom",
  delay = 300,
  disabled,
  triggerClassName = "inline-block max-w-full align-middle",
  contentClassName,
  children,
}: Props) {
  const [open, setOpen] = useState(false);
  const { context, floatingStyles, refs } = useFloating({
    open,
    onOpenChange: setOpen,
    placement,
    middleware: [offset(6), flip(), shift({ padding: 8 })],
  });
  const hover = useHover(context, { delay: { open: delay, close: 80 } });
  const { getReferenceProps, getFloatingProps } = useInteractions([hover]);

  if (disabled) {
    return children;
  }

  if (!label) {
    return children;
  }

  return (
    <>
      <span ref={refs.setReference} className={triggerClassName} {...getReferenceProps()}>
        {cloneElement(children)}
      </span>
      {open ? (
        <FloatingPortal preserveTabOrder={false}>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            className={`z-[10000] max-w-[260px] rounded-md border border-border bg-surface-popover px-2 py-1 text-[11.5px] font-medium text-ink-soft shadow-floating ${contentClassName ?? ""}`}
            {...getFloatingProps()}
          >
            {label}
          </div>
        </FloatingPortal>
      ) : null}
    </>
  );
}
