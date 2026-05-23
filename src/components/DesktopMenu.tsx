import { Dropdown, Separator } from "@heroui/react";
import { createContext, useContext, useId, type ComponentProps, type ReactElement, type ReactNode } from "react";
import type { Placement as FloatingPlacement } from "@floating-ui/react";

type DesktopMenuContextValue = {
  close: () => void;
};

const DesktopMenuContext = createContext<DesktopMenuContextValue | null>(null);

type DesktopMenuProps = {
  ariaLabel: string;
  children: ReactNode;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  placement?: FloatingPlacement;
  trigger: ReactElement<Record<string, unknown>>;
};

export function DesktopMenu({
  ariaLabel,
  children,
  isOpen,
  onOpenChange,
  placement = "bottom-end",
  trigger,
}: DesktopMenuProps) {
  const width = "min(240px, calc(100vw - 24px))";
  return (
    <DesktopMenuContext.Provider
      value={{
        close: () => onOpenChange(false),
      }}
    >
      <Dropdown isOpen={isOpen} onOpenChange={onOpenChange}>
        {renderMenuTrigger(trigger, ariaLabel)}
        <Dropdown.Popover
          placement={toHeroPlacement(placement)}
          offset={6}
          className="desktop-menu-popover w-[240px] rounded-[10px] border border-border-subtle bg-surface-popover p-1 text-ink shadow-floating"
          style={{ width }}
        >
          <Dropdown.Menu aria-label={ariaLabel} className="grid gap-0.5" shouldCloseOnSelect={false}>
            {children}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </DesktopMenuContext.Provider>
  );
}

export function DesktopMenuItem({
  children,
  disabled,
  label,
  onSelect,
}: {
  children: ReactNode;
  disabled?: boolean;
  label: string;
  onSelect?: () => void;
}) {
  const context = useContext(DesktopMenuContext);
  if (!context) {
    throw new Error("DesktopMenuItem must be used inside DesktopMenu");
  }
  const id = useId();
  return (
    <Dropdown.Item
      id={id}
      textValue={label}
      isDisabled={disabled}
      className="min-w-0 overflow-hidden"
      onAction={() => {
        if (disabled) return;
        onSelect?.();
        context.close();
      }}
    >
      <span className="min-w-0 flex-1 truncate">{children}</span>
    </Dropdown.Item>
  );
}

export function DesktopSubmenu({
  children,
  disabled,
  label,
}: {
  children: ReactNode;
  disabled?: boolean;
  label: string;
}) {
  const context = useContext(DesktopMenuContext);
  if (!context) {
    throw new Error("DesktopSubmenu must be used inside DesktopMenu");
  }
  const id = useId();
  return (
    <DesktopMenuContext.Provider value={context}>
      <Dropdown.SubmenuTrigger delay={80}>
        <Dropdown.Item
          id={id}
          textValue={label}
          isDisabled={disabled}
          className="min-w-0 justify-between overflow-hidden"
        >
          <span className="min-w-0 truncate">{label}</span>
          <Dropdown.SubmenuIndicator className="text-ink-faint" />
        </Dropdown.Item>
        <Dropdown.Popover
          placement="right top"
          offset={6}
          className="desktop-menu-popover w-[240px] rounded-[10px] border border-border-subtle bg-surface-popover p-1 text-ink shadow-floating"
          style={{ width: "min(240px, calc(100vw - 24px))" }}
        >
          <Dropdown.Menu aria-label={label} className="grid gap-0.5" shouldCloseOnSelect={false}>
            {children}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown.SubmenuTrigger>
    </DesktopMenuContext.Provider>
  );
}

export function DesktopMenuSectionLabel({ children }: { children: ReactNode }) {
  const id = useId();
  const label = typeof children === "string" ? children : undefined;
  return (
    <Dropdown.Item
      id={id}
      textValue={label}
      isDisabled
      className="min-w-0 truncate px-2 py-1 text-[11px] font-semibold text-ink-faint"
    >
      {children}
    </Dropdown.Item>
  );
}

export function DesktopMenuSeparator() {
  return <Separator className="my-1 h-px bg-border-subtle/70" />;
}

type HeroPlacement = NonNullable<ComponentProps<typeof Dropdown.Popover>["placement"]>;

function toHeroPlacement(placement: FloatingPlacement): HeroPlacement {
  if (placement === "right-start") return "right top";
  if (placement === "right-end") return "right bottom";
  if (placement === "left-start") return "left top";
  if (placement === "left-end") return "left bottom";
  return placement.replace("-", " ") as HeroPlacement;
}

function renderMenuTrigger(trigger: ReactElement<Record<string, unknown>>, ariaLabel: string) {
  const props = trigger.props;
  const label = typeof props["aria-label"] === "string" ? props["aria-label"] : ariaLabel;
  const className = typeof props.className === "string" ? props.className : undefined;
  return (
    <Dropdown.Trigger
      type="button"
      aria-label={label}
      isDisabled={props.disabled === true || props.isDisabled === true}
      className={`${className ?? ""} no-drag`}
    >
      {props.children as ReactNode}
    </Dropdown.Trigger>
  );
}
