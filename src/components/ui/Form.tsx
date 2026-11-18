import { forwardRef, useId, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TextareaHTMLAttributes } from 'react';
import { Checkbox as RadixCheckbox, Switch as RadixSwitch } from 'radix-ui';
import { Check, ChevronDown, Minus } from 'lucide-react';
import { cn } from '@/lib/cn';

const CONTROL =
  'w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink placeholder:text-faint transition-colors focus:border-brand disabled:bg-sunken disabled:text-faint';

export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean }>(function TextInput(
  { className, invalid, ...rest },
  ref,
) {
  return <input ref={ref} aria-invalid={invalid || undefined} className={cn(CONTROL, 'h-10', invalid && 'border-danger', className)} {...rest} />;
});

export const TextArea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(function TextArea({ className, ...rest }, ref) {
  return <textarea ref={ref} className={cn(CONTROL, 'min-h-[84px] resize-y py-2 leading-relaxed', className)} {...rest} />;
});

export function Select({ className, children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className={cn('relative inline-flex', className)}>
      <select className={cn(CONTROL, 'h-10 cursor-pointer appearance-none pr-9 font-medium')} {...rest}>
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute top-1/2 right-3 size-4 -translate-y-1/2 text-muted" aria-hidden />
    </span>
  );
}

export function Field({
  label,
  hint,
  error,
  optional,
  children,
}: {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  optional?: boolean;
  children: (props: { id: string; 'aria-describedby'?: string }) => ReactNode;
}) {
  const id = useId();
  const describedBy = error ? `${id}-error` : hint ? `${id}-hint` : undefined;
  return (
    <div>
      <label htmlFor={id} className="mb-1.5 flex items-baseline gap-2 text-sm font-semibold text-ink">
        {label}
        {optional && <span className="text-[12.5px] font-normal text-muted">ไม่บังคับ</span>}
      </label>
      {children({ id, 'aria-describedby': describedBy })}
      {error ? (
        <p id={`${id}-error`} role="alert" className="mt-1.5 text-[13px] text-danger">
          {error}
        </p>
      ) : (
        hint && (
          <p id={`${id}-hint`} className="mt-1.5 text-[13px] text-muted">
            {hint}
          </p>
        )
      )}
    </div>
  );
}

export function Switch({
  checked,
  onCheckedChange,
  label,
  description,
  disabled,
  testId,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
  testId?: string;
}) {
  const id = useId();
  return (
    <div className="flex items-start gap-3">
      <RadixSwitch.Root
        id={id}
        data-testid={testId}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className="relative mt-0.5 h-6 w-11 shrink-0 rounded-full bg-line-strong transition-colors data-[state=checked]:bg-brand disabled:cursor-not-allowed disabled:opacity-50"
      >
        <RadixSwitch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white shadow-card transition-transform data-[state=checked]:translate-x-[22px]" />
      </RadixSwitch.Root>
      <div className="min-w-0">
        <label htmlFor={id} className={cn('text-sm font-semibold', disabled ? 'text-muted' : 'text-ink')}>
          {label}
        </label>
        {description && <p className="text-[13px] text-muted">{description}</p>}
      </div>
    </div>
  );
}

export function Checkbox({
  checked,
  onCheckedChange,
  label,
  disabled,
  className,
  onClick,
  testId,
}: {
  checked: boolean | 'indeterminate';
  onCheckedChange: (checked: boolean) => void;
  /** ชื่อสำหรับ screen reader */
  label: string;
  disabled?: boolean;
  className?: string;
  onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  testId?: string;
}) {
  return (
    <RadixCheckbox.Root
      checked={checked}
      onCheckedChange={(value) => onCheckedChange(value === true)}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      data-testid={testId}
      className={cn(
        'flex size-[22px] shrink-0 items-center justify-center rounded-md border-2 border-line-strong bg-surface text-on-action transition-colors data-[state=checked]:border-action data-[state=checked]:bg-action data-[state=indeterminate]:border-action data-[state=indeterminate]:bg-action disabled:cursor-not-allowed disabled:opacity-45',
        className,
      )}
    >
      <RadixCheckbox.Indicator>
        {checked === 'indeterminate' ? <Minus className="size-4" strokeWidth={3} aria-hidden /> : <Check className="size-4" strokeWidth={3} aria-hidden />}
      </RadixCheckbox.Indicator>
    </RadixCheckbox.Root>
  );
}

export interface TabItem<T extends string> {
  value: T;
  label: string;
  badge?: ReactNode;
}

/** แท็บแบบแถวเดียว ใช้ลูกศรซ้าย/ขวาเลื่อนได้ตาม pattern ของ tablist */
export function Tabs<T extends string>({
  items,
  value,
  onChange,
  label,
  variant = 'underline',
  className,
  testId,
}: {
  items: TabItem<T>[];
  value: T;
  onChange: (value: T) => void;
  label: string;
  variant?: 'underline' | 'pill';
  className?: string;
  /** แต่ละแท็บได้ data-testid เป็น `${testId}-${value}` */
  testId?: string;
}) {
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
    const index = items.findIndex((item) => item.value === value);
    const next = items[(index + (event.key === 'ArrowRight' ? 1 : -1) + items.length) % items.length];
    if (!next) return;
    event.preventDefault();
    onChange(next.value);
    const buttons = event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]');
    buttons[items.indexOf(next)]?.focus();
  };
  return (
    <div
      role="tablist"
      aria-label={label}
      onKeyDown={onKeyDown}
      className={cn(
        'flex items-center',
        variant === 'underline' ? 'gap-1 border-b border-line' : 'inline-flex gap-0.5 rounded-control border border-line bg-surface p-0.5',
        className,
      )}
    >
      {items.map((item) => {
        const active = item.value === value;
        return (
          <button
            key={item.value}
            type="button"
            role="tab"
            aria-selected={active}
            data-testid={testId ? `${testId}-${item.value}` : undefined}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(item.value)}
            className={cn(
              'inline-flex items-center gap-2 text-sm font-semibold whitespace-nowrap transition-colors',
              variant === 'underline'
                ? cn('-mb-px h-11 rounded-t-lg border-b-2 px-4', active ? 'border-brand bg-brand-soft text-action' : 'border-transparent text-muted hover:text-ink')
                : cn('h-8 rounded-lg px-3.5', active ? 'bg-brand-soft text-action' : 'text-muted hover:text-ink'),
            )}
          >
            {item.label}
            {item.badge}
          </button>
        );
      })}
    </div>
  );
}
