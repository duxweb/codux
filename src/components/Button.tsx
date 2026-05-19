import { Button as HButton } from "@heroui/react";
import type { ComponentProps, ReactNode } from "react";
import { logTerminalFocusDebug } from "../debug/terminalFocusDebug";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

type IconLike = (props: { size?: number; strokeWidth?: number; className?: string }) => ReactNode;

type HButtonProps = ComponentProps<typeof HButton>;

type Props = {
  variant?: ButtonVariant;
  size?: ButtonSize;
  leading?: IconLike;
  trailing?: IconLike;
  block?: boolean;
  isIconOnly?: boolean;
  disabled?: boolean;
  excludeFromTabOrder?: HButtonProps["excludeFromTabOrder"];
  className?: string;
  children?: ReactNode;
  onPress?: HButtonProps["onPress"];
  onPressUp?: HButtonProps["onPressUp"];
  onPointerDown?: HButtonProps["onPointerDown"];
  onPointerDownCapture?: HButtonProps["onPointerDownCapture"];
  preventFocusOnPress?: HButtonProps["preventFocusOnPress"];
  type?: HButtonProps["type"];
  form?: HButtonProps["form"];
  autoFocus?: HButtonProps["autoFocus"];
  "aria-label"?: string;
};

const heroVariantMap: Record<ButtonVariant, NonNullable<HButtonProps["variant"]>> = {
  primary: "primary",
  secondary: "secondary",
  ghost: "ghost",
  danger: "danger",
};

const iconSizeMap: Record<ButtonSize, number> = {
  sm: 12,
  md: 14,
  lg: 15,
};

export function Button({
  variant = "secondary",
  size = "md",
  leading: Leading,
  trailing: Trailing,
  block,
  isIconOnly,
  disabled,
  excludeFromTabOrder = true,
  className,
  children,
  onPress,
  onPressUp,
  onPointerDownCapture,
  preventFocusOnPress = true,
  ...rest
}: Props) {
  const iconSize = iconSizeMap[size];
  const debugName =
    typeof rest["aria-label"] === "string"
      ? rest["aria-label"]
      : typeof children === "string"
        ? children
        : "button";

  return (
    <HButton
      variant={heroVariantMap[variant]}
      size={size}
      isIconOnly={isIconOnly}
      isDisabled={disabled}
      excludeFromTabOrder={excludeFromTabOrder}
      preventFocusOnPress={preventFocusOnPress}
      className={`${block ? "w-full" : ""} focus:outline-none focus-visible:outline-none ${className ?? ""}`}
      onPointerDownCapture={(event) => {
        logTerminalFocusDebug("button:pointerdown-capture", {
          name: debugName,
          active: document.activeElement?.tagName.toLowerCase(),
        });
        onPointerDownCapture?.(event);
      }}
      onPressUp={(event) => {
        onPressUp?.(event);
        logTerminalFocusDebug("button:onPressUp", {
          name: debugName,
          active: document.activeElement?.tagName.toLowerCase(),
        });
        if (!disabled && onPress) {
          logTerminalFocusDebug("button:onPress", {
            name: debugName,
            active: document.activeElement?.tagName.toLowerCase(),
          });
          onPress(event as Parameters<NonNullable<HButtonProps["onPress"]>>[0]);
        }
      }}
      {...rest}
    >
      {Leading ? <Leading size={iconSize} strokeWidth={2} /> : null}
      {children}
      {Trailing ? <Trailing size={iconSize} strokeWidth={2} /> : null}
    </HButton>
  );
}
