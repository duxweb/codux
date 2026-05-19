import { Tooltip as HTooltip } from "@heroui/react";
import { type ReactElement, type ReactNode } from "react";

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
  if (disabled) {
    return children;
  }

  if (!label) {
    return children;
  }

  return (
    <HTooltip delay={delay} closeDelay={80}>
      <HTooltip.Trigger className={triggerClassName}>
        {children}
      </HTooltip.Trigger>
      <HTooltip.Content
        placement={placement}
        showArrow={false}
        className={`max-w-[260px] rounded-md border border-line-strong bg-surface-chrome px-2 py-1 text-[11.5px] font-medium text-ink-soft shadow-pop ${contentClassName ?? ""}`}
      >
        {label}
      </HTooltip.Content>
    </HTooltip>
  );
}
