import type { CSSProperties } from "react";

export type AppIconStyleName = "default" | "cobalt" | "sunset" | "forest";

type AppIconPalette = {
  top: string;
  bottom: string;
};

const appIconPalettes: Record<AppIconStyleName, AppIconPalette> = {
  default: { top: "rgb(61 128 250)", bottom: "rgb(41 92 219)" },
  cobalt: { top: "rgb(31 36 51)", bottom: "rgb(20 23 33)" },
  sunset: { top: "rgb(245 107 82)", bottom: "rgb(224 77 66)" },
  forest: { top: "rgb(46 158 115)", bottom: "rgb(31 128 92)" },
};

export const appIconStyles = [
  { value: "default", labelKey: "settings.app_icon.option.default", label: "Default" },
  { value: "cobalt", labelKey: "settings.app_icon.option.cobalt", label: "Cobalt" },
  { value: "sunset", labelKey: "settings.app_icon.option.sunset", label: "Sunset" },
  { value: "forest", labelKey: "settings.app_icon.option.forest", label: "Forest" },
] satisfies Array<{ value: AppIconStyleName; labelKey: string; label: string }>;

export function isAppIconStyle(value: string): value is AppIconStyleName {
  return value === "default" || value === "cobalt" || value === "sunset" || value === "forest";
}

export function AppIconMark({
  styleName = "default",
  size = 64,
  className,
  style,
}: {
  styleName?: AppIconStyleName | string;
  size?: number;
  className?: string;
  style?: CSSProperties;
}) {
  const resolvedStyle = isAppIconStyle(styleName) ? styleName : "default";
  const palette = appIconPalettes[resolvedStyle];
  const id = `codux-app-icon-${resolvedStyle}`;

  return (
    <svg
      viewBox="0 0 128 128"
      width={size}
      height={size}
      className={className}
      style={style}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <linearGradient id={`${id}-bg`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={palette.top} />
          <stop offset="1" stopColor={palette.bottom} />
        </linearGradient>
        <radialGradient id={`${id}-top`} cx="50%" cy="18%" r="52%">
          <stop offset="0" stopColor="white" stopOpacity="0.1" />
          <stop offset="1" stopColor="white" stopOpacity="0" />
        </radialGradient>
        <radialGradient id={`${id}-bottom`} cx="50%" cy="92%" r="48%">
          <stop offset="0" stopColor="black" stopOpacity="0.08" />
          <stop offset="1" stopColor="black" stopOpacity="0" />
        </radialGradient>
        <filter id={`${id}-shadow`} x="-20%" y="-20%" width="140%" height="140%">
          <feDropShadow dx="0" dy="1.28" stdDeviation="1.28" floodColor="black" floodOpacity="0.2" />
        </filter>
        <clipPath id={`${id}-clip`}>
          <rect x="10.24" y="10.24" width="107.52" height="107.52" rx="28.16" ry="28.16" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${id}-clip)`}>
        <rect x="10.24" y="10.24" width="107.52" height="107.52" fill={`url(#${id}-bg)`} />
        <rect x="10.24" y="10.24" width="107.52" height="107.52" fill={`url(#${id}-top)`} />
        <rect x="10.24" y="10.24" width="107.52" height="107.52" fill={`url(#${id}-bottom)`} />
        <path
          d="M40.32 44.8 L62.08 64 L40.32 83.2"
          fill="none"
          stroke="white"
          strokeOpacity="0.4"
          strokeWidth="11.52"
          strokeLinecap="square"
          strokeLinejoin="miter"
        />
        <path
          d="M65.92 44.8 L87.68 64 L65.92 83.2"
          fill="none"
          stroke="white"
          strokeWidth="11.52"
          strokeLinecap="square"
          strokeLinejoin="miter"
          filter={`url(#${id}-shadow)`}
        />
        <rect
          x="10.74"
          y="10.74"
          width="106.52"
          height="106.52"
          rx="27.66"
          ry="27.66"
          fill="none"
          stroke="white"
          strokeOpacity="0.08"
          strokeWidth="1"
        />
      </g>
    </svg>
  );
}
