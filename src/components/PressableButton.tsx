import { useRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { mergeProps, usePress, type PressEvent } from "react-aria";

type Props = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onClick"> & {
  children?: ReactNode;
  excludeFromTabOrder?: boolean;
  onPress?: (event: PressEvent) => void;
  onPressUp?: (event: PressEvent) => void;
  preventFocusOnPress?: boolean;
};

export function PressableButton({
  children,
  disabled,
  excludeFromTabOrder = true,
  onPress,
  onPressUp,
  preventFocusOnPress = true,
  tabIndex,
  type = "button",
  ...props
}: Props) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const { pressProps } = usePress({
    ref,
    isDisabled: disabled,
    onPress,
    onPressUp,
    preventFocusOnPress,
  });

  return (
    <button
      {...mergeProps(props, pressProps)}
      ref={ref}
      type={type}
      disabled={disabled}
      tabIndex={excludeFromTabOrder ? -1 : tabIndex}
    >
      {children}
    </button>
  );
}
