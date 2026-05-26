import { forwardRef, useCallback, useRef, type ButtonHTMLAttributes, type MutableRefObject, type ReactNode, type Ref } from "react";
import { mergeProps, useHover, usePress } from "react-aria";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "success";
export type ButtonSize = "sm" | "md" | "lg";

type IconLike = (props: { size?: number; strokeWidth?: number; className?: string }) => ReactNode;

type Props = {
  variant?: ButtonVariant;
  size?: ButtonSize;
  leading?: IconLike;
  trailing?: IconLike;
  block?: boolean;
  isIconOnly?: boolean;
  disabled?: boolean;
  isDisabled?: boolean;
  excludeFromTabOrder?: boolean;
  tabIndex?: ButtonHTMLAttributes<HTMLButtonElement>["tabIndex"];
  className?: string;
  children?: ReactNode;
  onPress?: () => void;
  onPressUp?: () => void;
  preventFocusOnPress?: boolean;
  type?: ButtonHTMLAttributes<HTMLButtonElement>["type"];
  form?: ButtonHTMLAttributes<HTMLButtonElement>["form"];
  autoFocus?: ButtonHTMLAttributes<HTMLButtonElement>["autoFocus"];
  "aria-label"?: string;
};

const variantClassNameMap: Record<ButtonVariant, string> = {
  primary: "codux-button-primary",
  secondary: "codux-button-secondary",
  ghost: "codux-button-ghost",
  danger: "codux-button-danger",
  success: "codux-button-success",
};

const sizeClassNameMap: Record<ButtonSize, string> = {
  sm: "codux-button-sm",
  md: "codux-button-md",
  lg: "codux-button-lg",
};

const iconOnlyClassNameMap: Record<ButtonSize, string> = {
  sm: "codux-button-icon-sm",
  md: "codux-button-icon-md",
  lg: "codux-button-icon-lg",
};

const iconSizeMap: Record<ButtonSize, number> = {
  sm: 12,
  md: 14,
  lg: 15,
};

const inlineIconClassNameMap: Record<ButtonSize, string> = {
  sm: "codux-button-inline-icon-sm",
  md: "codux-button-inline-icon-md",
  lg: "codux-button-inline-icon-lg",
};

export const Button = forwardRef<HTMLButtonElement, Props>(function Button({
  variant = "secondary",
  size = "md",
  leading: Leading,
  trailing: Trailing,
  block,
  isIconOnly,
  disabled,
  isDisabled,
  excludeFromTabOrder = true,
  className,
  children,
  onPress,
  onPressUp,
  preventFocusOnPress = false,
  type = "button",
  tabIndex,
  ...rest
}, ref) {
  const iconSize = iconSizeMap[size];
  const isDisabledState = disabled || isDisabled;
  const localRef = useRef<HTMLButtonElement | null>(null);
  const pressStartedRef = useRef(false);
  const mergedRef = useMergedRef(localRef, ref);
  const { hoverProps, isHovered } = useHover({ isDisabled: isDisabledState });
  const { pressProps } = usePress({
    ref: localRef,
    isDisabled: isDisabledState,
    onPressStart: () => {
      pressStartedRef.current = true;
    },
    onPress: (event) => {
      pressStartedRef.current = false;
      onPress?.();
    },
    onPressUp: (event) => {
      const hadPressStart = pressStartedRef.current;
      onPressUp?.();
      if (!hadPressStart) {
        onPress?.();
      }
    },
    preventFocusOnPress,
  });

  return (
    <button
      {...mergeProps(rest, hoverProps, pressProps)}
      ref={mergedRef}
      type={type}
      disabled={isDisabledState}
      data-codux-button="true"
      data-hovered={isHovered ? "true" : undefined}
      tabIndex={tabIndex ?? (excludeFromTabOrder ? -1 : undefined)}
      className={`codux-button ${isIconOnly ? iconOnlyClassNameMap[size] : sizeClassNameMap[size]} ${block ? "w-full" : ""} ${variantClassNameMap[variant]} ${className ?? ""}`}
    >
      {Leading ? <Leading size={iconSize} strokeWidth={2} className={inlineIconClassNameMap[size]} /> : null}
      {children}
      {Trailing ? <Trailing size={iconSize} strokeWidth={2} className={inlineIconClassNameMap[size]} /> : null}
    </button>
  );
});

function useMergedRef<T>(localRef: MutableRefObject<T | null>, forwardedRef: Ref<T> | undefined) {
  return useCallback(
    (node: T | null) => {
      localRef.current = node;
      if (typeof forwardedRef === "function") {
        forwardedRef(node);
      } else if (forwardedRef) {
        forwardedRef.current = node;
      }
    },
    [forwardedRef, localRef],
  );
}
