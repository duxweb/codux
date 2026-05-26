import { createContext, useContext, useEffect, useRef, type HTMLAttributes, type ReactNode } from "react";

type ModalContextValue = {
  onOpenChange?: (open: boolean) => void;
};

const ModalContext = createContext<ModalContextValue>({});

type ModalRootProps = {
  isOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
};

export function Modal({ isOpen, onOpenChange, children }: ModalRootProps) {
  useEffect(() => {
    if (!isOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onOpenChange?.(false);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [isOpen, onOpenChange]);

  if (!isOpen) return null;

  return <ModalContext.Provider value={{ onOpenChange }}>{children}</ModalContext.Provider>;
}

function Backdrop({ children, className, onPointerDown, onPointerUp, ...props }: HTMLAttributes<HTMLDivElement>) {
  const context = useContext(ModalContext);
  const backdropPointerDownRef = useRef(false);
  return (
    <div
      className={className}
      onPointerDown={(event) => {
        onPointerDown?.(event);
        backdropPointerDownRef.current = !event.defaultPrevented && event.target === event.currentTarget;
      }}
      onPointerUp={(event) => {
        onPointerUp?.(event);
        if (backdropPointerDownRef.current && !event.defaultPrevented && event.target === event.currentTarget) {
          context.onOpenChange?.(false);
        }
        backdropPointerDownRef.current = false;
      }}
      {...props}
    >
      {children}
    </div>
  );
}

function Container({ children, className, ...props }: HTMLAttributes<HTMLDivElement> & { size?: string; placement?: string }) {
  return (
    <div className={className} {...props}>
      {children}
    </div>
  );
}

function Dialog({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div role="dialog" aria-modal="true" className={className} {...props}>
      {children}
    </div>
  );
}

function Header({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={className} {...props}>
      {children}
    </div>
  );
}

function Heading({ children, className, ...props }: HTMLAttributes<HTMLHeadingElement>) {
  return (
    <h2 className={className} {...props}>
      {children}
    </h2>
  );
}

function Footer({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={className} {...props}>
      {children}
    </div>
  );
}

Modal.Backdrop = Backdrop;
Modal.Container = Container;
Modal.Dialog = Dialog;
Modal.Header = Header;
Modal.Heading = Heading;
Modal.Footer = Footer;
