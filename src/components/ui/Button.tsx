import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { LoaderCircle } from 'lucide-react';
import { cn } from '@/lib/cn';

type Variant = 'primary' | 'secondary' | 'outline' | 'ghost' | 'danger';
type Size = 'sm' | 'md' | 'lg';

const VARIANT: Record<Variant, string> = {
  primary:
    'bg-action text-on-action shadow-card hover:bg-action-hover disabled:bg-line-strong disabled:text-surface disabled:shadow-none',
  secondary:
    'bg-surface text-ink border border-line-strong shadow-card hover:bg-sunken disabled:text-faint disabled:bg-surface',
  outline:
    'bg-surface text-action border border-action/60 hover:bg-brand-soft disabled:text-faint disabled:border-line',
  ghost: 'text-ink hover:bg-sunken disabled:text-faint disabled:hover:bg-transparent',
  danger: 'bg-surface text-danger border border-danger/40 hover:bg-danger-soft disabled:text-faint disabled:border-line',
};

const SIZE: Record<Size, string> = {
  sm: 'h-8 px-3 text-[13px] gap-1.5 rounded-lg',
  md: 'h-10 px-4 text-sm gap-2 rounded-control',
  lg: 'h-11 px-5 text-[15px] gap-2 rounded-control',
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  icon?: ReactNode;
  /** แสดงตัวหมุนและกันกดซ้ำระหว่างรอผลจาก backend */
  busy?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'secondary', size = 'md', icon, busy, className, children, disabled, type = 'button', ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      className={cn(
        'inline-flex shrink-0 items-center justify-center font-semibold whitespace-nowrap transition-colors select-none',
        VARIANT[variant],
        SIZE[size],
        className,
      )}
      {...rest}
    >
      {busy ? <LoaderCircle className="size-4 animate-spin" aria-hidden /> : icon}
      {children}
    </button>
  );
});

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** บังคับมี label ให้ screen reader เพราะปุ่มไม่มีข้อความ */
  label: string;
  size?: 'sm' | 'md';
  tone?: 'default' | 'onDark';
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, size = 'md', tone = 'default', className, children, type = 'button', ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      aria-label={label}
      title={label}
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-lg transition-colors disabled:opacity-40',
        size === 'sm' ? 'size-8' : 'size-10',
        tone === 'default' ? 'text-muted hover:bg-sunken hover:text-ink' : 'text-white/85 hover:bg-white/15 hover:text-white',
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
});
