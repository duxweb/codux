import { forwardRef } from "react";
import type { ChangeEvent, FormHTMLAttributes, InputHTMLAttributes, ReactNode, TextareaHTMLAttributes } from "react";

export function SettingsForm({
  children,
  className,
  ...props
}: FormHTMLAttributes<HTMLFormElement> & { children: ReactNode; className?: string }) {
  return (
    <form className={`grid gap-4 ${className ?? ""}`} {...props}>
      {children}
    </form>
  );
}

export function Field({
  label,
  description,
  hint,
  required,
  fullWidth,
  children,
}: {
  label?: ReactNode;
  description?: ReactNode;
  hint?: ReactNode;
  required?: boolean;
  fullWidth?: boolean;
  children: ReactNode;
}) {
  if (fullWidth) {
    return (
      <label className="grid gap-2">
        {label !== undefined && (
          <div className="min-w-0">
            <div className="text-sm font-medium text-ink">
              {label}
              {required && <span className="text-brand-red ml-0.5">*</span>}
            </div>
            {description !== undefined && <div className="mt-0.5 text-xs text-ink-faint">{description}</div>}
            {hint !== undefined && <div className="mt-0.5 text-xs text-ink-faint">{hint}</div>}
          </div>
        )}
        <div className="min-w-0">{children}</div>
      </label>
    );
  }

  return (
    <label className="flex items-center justify-between gap-4">
      <div className="min-w-0 flex-1">
        {label !== undefined && (
          <div className="text-sm font-medium text-ink">
            {label}
            {required && <span className="text-brand-red ml-0.5">*</span>}
          </div>
        )}
        {description !== undefined && <div className="mt-0.5 text-xs text-ink-faint">{description}</div>}
        {hint !== undefined && <div className="mt-0.5 text-xs text-ink-faint">{hint}</div>}
      </div>
      <div className="flex min-w-[180px] w-[30%] max-w-[260px] flex-shrink-0 justify-end">{children}</div>
    </label>
  );
}

type TextInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "size"> & {
  fullWidth?: boolean;
  variant?: string;
};

export const TextInput = forwardRef<HTMLInputElement, TextInputProps>(function TextInput(
  { className, fullWidth = true, variant: _variant, ...props },
  ref,
) {
  return (
    <input
      ref={ref}
      className={`${fullWidth ? "w-full" : ""} min-h-9 rounded-[8px] border border-field-border bg-field-background px-3 text-sm text-field-foreground outline-none transition-colors placeholder:text-field-placeholder hover:border-field-border-hover hover:bg-field-hover focus:border-field-border-focus focus:bg-field-focus disabled:cursor-default disabled:opacity-55 ${className ?? ""}`}
      {...props}
    />
  );
});

type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & {
  fullWidth?: boolean;
  variant?: string;
};

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
  { className, fullWidth = true, variant: _variant, ...props },
  ref,
) {
  return (
    <textarea
      ref={ref}
      className={`${fullWidth ? "w-full" : ""} min-h-20 rounded-[8px] border border-field-border bg-field-background px-3 py-2 text-sm text-field-foreground outline-none transition-colors placeholder:text-field-placeholder hover:border-field-border-hover hover:bg-field-hover focus:border-field-border-focus focus:bg-field-focus disabled:cursor-default disabled:opacity-55 ${className ?? ""}`}
      {...props}
    />
  );
});

export function Select({
  options,
  value,
  defaultValue,
  onChange,
  placeholder,
  ariaLabel,
  className,
  isDisabled,
  disabled,
}: {
  options: { value: string; label: string }[];
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  className?: string;
  isDisabled?: boolean;
  disabled?: boolean;
}) {
  return (
    <select
      aria-label={ariaLabel}
      value={value}
      defaultValue={defaultValue}
      onChange={(event) => onChange?.(event.currentTarget.value)}
      disabled={isDisabled || disabled}
      className={`w-full min-h-9 appearance-none rounded-[8px] border border-field-border bg-field-background px-3 pr-8 text-sm text-field-foreground outline-none transition-colors hover:border-field-border-hover hover:bg-field-hover focus:border-field-border-focus focus:bg-field-focus disabled:cursor-default disabled:opacity-55 ${className ?? ""}`}
    >
      {placeholder ? <option value="">{placeholder}</option> : null}
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
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
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={`relative h-6 w-10 shrink-0 rounded-full border border-border-subtle transition-colors disabled:opacity-50 ${checked ? "bg-brand-blue" : "bg-fill/12"}`}
      onClick={() => onChange?.(!checked)}
    >
      <span
        className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white shadow-sm transition-transform ${checked ? "translate-x-4" : "translate-x-0"}`}
      />
    </button>
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
    <label className={`inline-flex min-w-0 items-center gap-2 text-sm text-ink-soft ${disabled ? "opacity-50" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event: ChangeEvent<HTMLInputElement>) => onChange?.(event.currentTarget.checked)}
        className="h-4 w-4 accent-brand-blue"
      />
      {label !== undefined && (
        <span className="min-w-0">{label}</span>
      )}
    </label>
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
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium text-ink">{label}</div>
        {description !== undefined && <div className="text-xs text-ink-faint mt-0.5">{description}</div>}
      </div>
      <div className="flex flex-shrink-0 justify-end">{children}</div>
    </div>
  );
}

export function SettingsCard({
  title,
  description,
  action,
  children,
  className,
}: {
  title?: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {(title !== undefined || description !== undefined || action !== undefined) && (
        <div className="mb-2 px-1">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              {title !== undefined && <h3 className="text-sm font-semibold text-ink">{title}</h3>}
              {description !== undefined && <p className="mt-1 text-xs text-ink-faint">{description}</p>}
            </div>
            {action !== undefined && <div className="flex flex-none items-center justify-end">{action}</div>}
          </div>
        </div>
      )}
      <div className="settings-card-surface overflow-hidden rounded-[10px] px-3.5">
        <div className="divide-y divide-border-subtle/45 [&>*]:py-3.5 [&>*:first-child]:pt-3.5 [&>*:last-child]:pb-3.5">
          {children}
        </div>
      </div>
    </section>
  );
}

export const FormCard = SettingsCard;
