import {
  Checkbox as HCheckbox,
  Form as HForm,
  Input as HInput,
  Label as HLabel,
  ListBox as HListBox,
  Select as HSelect,
  Switch as HSwitch,
  TextArea as HTextArea,
} from "@heroui/react";
import type { ComponentProps, ReactNode } from "react";

export function SettingsForm({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <HForm className={`grid gap-4 ${className ?? ""}`}>
      {children}
    </HForm>
  );
}

export function Field({
  label,
  description,
  hint,
  required,
  children,
}: {
  label?: ReactNode;
  description?: ReactNode;
  hint?: ReactNode;
  required?: boolean;
  children: ReactNode;
}) {
  return (
    <label className="flex items-center justify-between gap-4">
      <div className="min-w-0 flex-1">
        {label !== undefined && (
          <div className="text-sm font-medium text-ink">
            {label}
            {required && <span className="text-brand-red ml-0.5">*</span>}
          </div>
        )}
        {description !== undefined && (
          <div className="mt-0.5 text-xs text-ink-faint">{description}</div>
        )}
        {hint !== undefined && (
          <div className="mt-0.5 text-xs text-ink-faint">{hint}</div>
        )}
      </div>
      <div className="min-w-[220px] w-[36%] max-w-[36%] flex-shrink-0">{children}</div>
    </label>
  );
}

export function TextInput(props: ComponentProps<typeof HInput>) {
  return <HInput {...props} />;
}

export function Textarea(props: ComponentProps<typeof HTextArea>) {
  return <HTextArea {...props} />;
}

export function Select({
  options,
  value,
  defaultValue,
  onChange,
  placeholder,
  ariaLabel,
  className,
  isDisabled,
}: {
  options: { value: string; label: string }[];
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  className?: string;
  isDisabled?: boolean;
}) {
  return (
    <HSelect
      aria-label={ariaLabel}
      selectedKey={value}
      defaultSelectedKey={defaultValue}
      onSelectionChange={(key) => {
        if (typeof key === "string" && onChange) onChange(key);
      }}
      placeholder={placeholder}
      className={className}
      isDisabled={isDisabled}
    >
      <HSelect.Trigger>
        <HSelect.Value />
        <HSelect.Indicator />
      </HSelect.Trigger>
      <HSelect.Popover>
        <HListBox>
          {options.map((option) => (
            <HListBox.Item
              key={option.value}
              id={option.value}
              textValue={option.label}
            >
              {option.label}
              <HListBox.ItemIndicator />
            </HListBox.Item>
          ))}
        </HListBox>
      </HSelect.Popover>
    </HSelect>
  );
}

export function Toggle({
  checked,
  onChange,
  disabled,
}: {
  checked: boolean;
  onChange?: (next: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <HSwitch
      isSelected={checked}
      onChange={(value) => onChange?.(value)}
      isDisabled={disabled}
      size="md"
    >
      <HSwitch.Control>
        <HSwitch.Thumb />
      </HSwitch.Control>
    </HSwitch>
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange?: (next: boolean) => void;
  label?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <HCheckbox
      isSelected={checked}
      onChange={(value) => onChange?.(value)}
      isDisabled={disabled}
    >
      <HCheckbox.Control>
        <HCheckbox.Indicator />
      </HCheckbox.Control>
      {label !== undefined && (
        <HCheckbox.Content>
          <HLabel>{label}</HLabel>
        </HCheckbox.Content>
      )}
    </HCheckbox>
  );
}

export function FormRow({
  label,
  description,
  children,
}: {
  label: ReactNode;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <div className="min-w-0">
        <div className="text-sm font-medium text-ink">{label}</div>
        {description !== undefined && (
          <div className="text-xs text-ink-faint mt-0.5">{description}</div>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

export function SettingsCard({
  title,
  description,
  children,
  className,
}: {
  title?: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {(title !== undefined || description !== undefined) && (
        <div className="mb-2 px-1">
          <div className="min-w-0">
            {title !== undefined && (
              <h3 className="text-sm font-semibold text-ink">{title}</h3>
            )}
            {description !== undefined && (
              <p className="mt-1 text-xs text-ink-faint">{description}</p>
            )}
          </div>
        </div>
      )}
      <div className="overflow-hidden rounded-[10px] bg-surface-card/30 px-3.5">
        <div className="divide-y divide-line/70 [&>*]:py-3.5 [&>*:first-child]:pt-3.5 [&>*:last-child]:pb-3.5">
          {children}
        </div>
      </div>
    </section>
  );
}

export const FormCard = SettingsCard;
