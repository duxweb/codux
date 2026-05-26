import {
  forwardRef,
  useCallback,
  useRef,
  type ButtonHTMLAttributes,
  type MutableRefObject,
  type ReactNode,
  type Ref,
} from "react";
import { mergeProps, useHover, usePress, type PressEvent } from "react-aria";

type Props = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onClick"> & {
  children?: ReactNode;
  excludeFromTabOrder?: boolean;
  onPressStart?: (event: PressEvent) => void;
  onPress?: (event: PressEvent) => void;
  onPressUp?: (event: PressEvent) => void;
  preventFocusOnPress?: boolean;
};

export const PressableButton = forwardRef<HTMLButtonElement, Props>(function PressableButton({
  children,
  disabled,
  excludeFromTabOrder = true,
  onPressStart,
  onPress,
  onPressUp,
  preventFocusOnPress = false,
  tabIndex,
  type = "button",
  ...props
}: Props, forwardedRef) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const pressStartedRef = useRef(false);
  const setRef = useMergedRef(ref, forwardedRef);
  const { hoverProps, isHovered } = useHover({ isDisabled: disabled });
  const { pressProps } = usePress({
    ref,
    isDisabled: disabled,
    onPressStart: (event) => {
      pressStartedRef.current = true;
      onPressStart?.(event);
    },
    onPress: (event) => {
      pressStartedRef.current = false;
      onPress?.(event);
    },
    onPressUp: (event) => {
      const hadPressStart = pressStartedRef.current;
      onPressUp?.(event);
      if (!hadPressStart) {
        onPress?.(event);
      }
    },
    preventFocusOnPress,
  });

  return (
    <button
      {...mergeProps(props, hoverProps, pressProps)}
      ref={setRef}
      type={type}
      disabled={disabled}
      data-codux-button="true"
      data-hovered={isHovered ? "true" : undefined}
      tabIndex={tabIndex ?? (excludeFromTabOrder ? -1 : undefined)}
    >
      {children}
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
