import {
  forwardRef,
  useCallback,
  useRef,
  type ButtonHTMLAttributes,
  type MutableRefObject,
  type ReactNode,
  type Ref,
} from "react";
import { mergeProps, usePress, type PressEvent } from "react-aria";

type Props = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onClick"> & {
  children?: ReactNode;
  excludeFromTabOrder?: boolean;
  onPress?: (event: PressEvent) => void;
  onPressUp?: (event: PressEvent) => void;
  preventFocusOnPress?: boolean;
};

export const PressableButton = forwardRef<HTMLButtonElement, Props>(function PressableButton({
  children,
  disabled,
  excludeFromTabOrder = true,
  onPress,
  onPressUp,
  preventFocusOnPress = true,
  tabIndex,
  type = "button",
  ...props
}: Props, forwardedRef) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const setRef = useMergedRef(ref, forwardedRef);
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
      ref={setRef}
      type={type}
      disabled={disabled}
      data-codux-button="true"
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
