import type { HTMLAttributes, ReactNode } from "react";

type SpinnerProps = HTMLAttributes<HTMLSpanElement> & {
  size?: "sm" | "md" | "lg";
  color?: string;
};

export function Spinner({ size = "md", className, ...props }: SpinnerProps) {
  const sizeClass = size === "sm" ? "h-3.5 w-3.5" : size === "lg" ? "h-5 w-5" : "h-4 w-4";
  return (
    <span
      aria-hidden="true"
      className={`inline-block ${sizeClass} animate-spin rounded-full border-2 border-current/25 border-t-current ${className ?? ""}`}
      {...props}
    />
  );
}

type ProgressBarProps = HTMLAttributes<HTMLDivElement> & {
  value?: number;
  maxValue?: number;
  isIndeterminate?: boolean;
  size?: string;
  color?: string;
  children?: ReactNode;
};

export function ProgressBar({
  value,
  maxValue = 100,
  isIndeterminate,
  className,
  children,
  size: _size,
  color: _color,
  ...props
}: ProgressBarProps) {
  const percent = Math.max(0, Math.min(100, ((value ?? 0) / maxValue) * 100));
  return (
    <div
      className={className}
      data-indeterminate={isIndeterminate ? "true" : undefined}
      style={{ "--progress-value": `${percent}%` } as React.CSSProperties}
      {...props}
    >
      {children}
    </div>
  );
}

function Track({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={className} {...props}>
      {children}
    </div>
  );
}

function Fill({ className, style, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={className}
      style={{ width: "var(--progress-value, 0%)", ...style }}
      {...props}
    />
  );
}

ProgressBar.Track = Track;
ProgressBar.Fill = Fill;
