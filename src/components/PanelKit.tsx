import type { ReactNode } from "react";
import { PressableButton } from "./PressableButton";
import { Tooltip, type TooltipPlacement } from "./Tooltip";

type IconLike = (props: { size?: number; strokeWidth?: number; className?: string }) => ReactNode;

export type PanelTone = "neutral" | "info" | "success" | "warning" | "danger";

export function PanelHeader({ title, trailing }: { title: ReactNode; trailing?: ReactNode }) {
  return (
    <div className="h-[42px] px-3.5 flex items-center justify-between flex-shrink-0 border-b border-line">
      <div className="text-sm font-semibold text-ink truncate">{title}</div>
      {trailing ? <div className="flex items-center gap-1.5">{trailing}</div> : null}
    </div>
  );
}

export function PanelCard({
  title,
  trailing,
  divider,
  children,
  bodyPadding = true,
  className,
}: {
  title?: ReactNode;
  trailing?: ReactNode;
  divider?: boolean;
  children: ReactNode;
  bodyPadding?: boolean;
  className?: string;
}) {
  const needsClip = divider || bodyPadding === false;
  return (
    <div
      className={`rounded-[10px] border border-line bg-fill/[0.035] ${needsClip ? "overflow-hidden" : ""} ${className ?? ""}`}
    >
      {title !== undefined && (
        <div
          className={`flex items-center justify-between gap-2 px-3 ${
            divider ? "h-[34px] border-b border-line" : "pt-3 pb-0"
          }`}
        >
          <div className="text-xs font-semibold text-ink truncate">{title}</div>
          {trailing}
        </div>
      )}
      <div className={bodyPadding ? "p-3" : ""}>{children}</div>
    </div>
  );
}

export function PanelSection({
  title,
  children,
  className,
}: {
  title?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {title !== undefined && (
        <div className="px-3.5 py-2 text-xs font-semibold tracking-wide text-ink-faint border-y border-line bg-fill/[0.02]">
          {title}
        </div>
      )}
      {children}
    </section>
  );
}

const toneClass: Record<PanelTone, string> = {
  neutral: "bg-fill/[0.025] border-t border-line text-ink-soft",
  info: "bg-brand-blue text-on-brand",
  success: "bg-brand-green text-on-brand",
  warning: "bg-brand-amber text-on-brand",
  danger: "bg-brand-red text-on-brand",
};

export function PanelStatusBar({
  tone = "neutral",
  leading,
  trailing,
}: {
  tone?: PanelTone;
  leading?: ReactNode;
  trailing?: ReactNode;
}) {
  return (
    <div className={`h-[36px] flex-shrink-0 px-3 flex items-center justify-between gap-2 text-xs ${toneClass[tone]}`}>
      <div className="flex items-center gap-1.5 min-w-0">{leading}</div>
      <div className="flex items-center gap-1 flex-shrink-0">{trailing}</div>
    </div>
  );
}

type IconBtnProps = {
  icon: IconLike;
  tooltip: string;
  placement?: TooltipPlacement;
  active?: boolean;
  busy?: boolean;
  disabled?: boolean;
  size?: number;
  onClick?: () => void;
};

export function PanelIconButton({
  icon: Icon,
  tooltip,
  placement = "bottom",
  active,
  busy,
  disabled,
  size = 13,
  onClick,
}: IconBtnProps) {
  const stateClass = disabled
    ? "text-ink-mute bg-fill/6 cursor-default opacity-75"
    : active
      ? "text-ink bg-fill/10"
      : "text-ink-mute hover:text-ink hover:bg-fill/8";

  return (
    <Tooltip label={tooltip} placement={placement}>
      <PressableButton
        onPressUp={onClick}
        disabled={disabled}
        className={`w-[24px] h-[24px] grid place-items-center rounded-md transition-colors ${stateClass}`}
      >
        <Icon size={size} strokeWidth={2} className={busy ? "animate-spin" : undefined} />
      </PressableButton>
    </Tooltip>
  );
}

export function PanelButton({
  tone = "neutral",
  leading: Leading,
  children,
  onClick,
}: {
  tone?: "neutral" | "ghost";
  leading?: IconLike;
  children: ReactNode;
  onClick?: () => void;
}) {
  const base = "h-6 px-2 inline-flex items-center gap-1 rounded-md transition-colors text-xs";
  const variant =
    tone === "ghost"
      ? "text-current/80 hover:text-current hover:bg-fill/10"
      : "text-ink-soft hover:text-ink hover:bg-fill/8";
  return (
    <PressableButton onPressUp={onClick} className={`${base} ${variant}`}>
      {Leading ? <Leading size={12} /> : null}
      {children}
    </PressableButton>
  );
}

export function PanelEmptyState({
  icon: Icon,
  title,
  description,
  action,
  tone = "info",
}: {
  icon: IconLike;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  tone?: "info" | "success" | "warning";
}) {
  const ring =
    tone === "info"
      ? "bg-brand-blue/12 text-brand-blue"
      : tone === "success"
        ? "bg-brand-green/12 text-brand-green"
        : "bg-brand-amber/12 text-brand-amber";
  return (
    <div className="flex-1 grid place-items-center text-center px-6">
      <div>
        <div className={`w-14 h-14 rounded-full mx-auto grid place-items-center mb-3 ${ring}`}>
          <Icon size={22} />
        </div>
        <div className="text-sm font-semibold">{title}</div>
        {description && (
          <div className="text-xs text-ink-mute mt-1.5 max-w-[220px] mx-auto leading-relaxed">{description}</div>
        )}
        {action && <div className="mt-4">{action}</div>}
      </div>
    </div>
  );
}
