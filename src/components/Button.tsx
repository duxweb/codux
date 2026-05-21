import { Button as HButton } from "@heroui/react";
import type { ComponentProps, ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "success";
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
  success: "primary",
};

const variantClassNameMap: Partial<Record<ButtonVariant, string>> = {
  success: "bg-brand-green text-on-brand hover:bg-brand-green/90 active:bg-brand-green/80",
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

  return (
    <HButton
      variant={heroVariantMap[variant]}
      size={size}
      isIconOnly={isIconOnly}
      isDisabled={disabled}
      excludeFromTabOrder={excludeFromTabOrder}
      preventFocusOnPress={preventFocusOnPress}
      className={`${block ? "w-full" : ""} focus:outline-none focus-visible:outline-none ${variantClassNameMap[variant] ?? ""} ${className ?? ""}`}
      onPointerDownCapture={(event) => {
        onPointerDownCapture?.(event);
      }}
      onPressUp={(event) => {
        onPressUp?.(event);
        if (!disabled && onPress) {
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
