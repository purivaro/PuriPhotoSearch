import type { ReactNode } from 'react';
import { CircleAlert, LoaderCircle, RefreshCw } from 'lucide-react';
import { cn } from '@/lib/cn';
import { errorMessage } from '@/lib/format';
import { Button } from './Button';

export function Spinner({ className }: { className?: string }) {
  return <LoaderCircle className={cn('size-5 animate-spin text-brand', className)} aria-hidden />;
}

export function LoadingBlock({ label = 'กำลังโหลด…', className }: { label?: string; className?: string }) {
  return (
    <div role="status" aria-live="polite" className={cn('flex items-center justify-center gap-3 py-16 text-muted', className)}>
      <Spinner />
      <span>{label}</span>
    </div>
  );
}

interface EmptyStateProps {
  icon?: ReactNode;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
  compact?: boolean;
}

export function EmptyState({ icon, title, description, action, className, compact }: EmptyStateProps) {
  return (
    <div className={cn('flex flex-col items-center justify-center text-center', compact ? 'px-4 py-8' : 'px-6 py-16', className)}>
      {icon && (
        <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-brand-soft text-action [&>svg]:size-7">{icon}</div>
      )}
      <h3 className={cn('font-bold text-ink', compact ? 'text-[15px]' : 'text-lg')}>{title}</h3>
      {description && <p className="mt-1.5 max-w-[460px] text-sm text-muted">{description}</p>}
      {action && <div className="mt-5 flex flex-wrap items-center justify-center gap-2.5">{action}</div>}
    </div>
  );
}

interface ErrorStateProps {
  error: unknown;
  /** หัวข้อบอกว่าส่วนใดโหลดไม่ได้ */
  title?: string;
  onRetry?: () => void;
  className?: string;
  compact?: boolean;
  extraAction?: ReactNode;
}

export function ErrorState({ error, title = 'โหลดข้อมูลส่วนนี้ไม่สำเร็จ', onRetry, className, compact, extraAction }: ErrorStateProps) {
  return (
    <div role="alert" className={cn('flex flex-col items-center justify-center text-center', compact ? 'px-4 py-6' : 'px-6 py-14', className)}>
      <div className="mb-3 flex size-12 items-center justify-center rounded-2xl bg-danger-soft text-danger">
        <CircleAlert className="size-6" aria-hidden />
      </div>
      <h3 className="text-[15px] font-bold text-ink">{title}</h3>
      <p className="mt-1 max-w-[460px] text-sm text-muted">{errorMessage(error)}</p>
      {(onRetry || extraAction) && (
        <div className="mt-4 flex flex-wrap items-center justify-center gap-2.5">
          {onRetry && (
            <Button size="sm" icon={<RefreshCw className="size-4" aria-hidden />} onClick={onRetry}>
              ลองใหม่
            </Button>
          )}
          {extraAction}
        </div>
      )}
    </div>
  );
}

export function InlineNotice({
  tone = 'info',
  icon,
  children,
  action,
  className,
}: {
  tone?: 'info' | 'warn' | 'danger' | 'ok' | 'brand';
  icon?: ReactNode;
  children: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  const toneClass = {
    info: 'bg-info-soft text-info border-info/25',
    warn: 'bg-warn-soft text-warn border-warn/25',
    danger: 'bg-danger-soft text-danger border-danger/25',
    ok: 'bg-ok-soft text-ok border-ok/25',
    brand: 'bg-brand-soft text-action border-brand-line',
  }[tone];
  return (
    <div className={cn('flex items-start gap-2.5 rounded-xl border px-3.5 py-2.5 text-sm', toneClass, className)}>
      {icon && <span className="mt-0.5 shrink-0 [&>svg]:size-[18px]">{icon}</span>}
      <div className="min-w-0 flex-1">{children}</div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

export function ProgressBar({ value, max, indeterminate, label }: { value: number; max: number; indeterminate?: boolean; label: string }) {
  const ratio = max > 0 ? Math.min(1, Math.max(0, value / max)) : 0;
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={indeterminate ? undefined : max}
      aria-valuenow={indeterminate ? undefined : value}
      className="relative h-2 w-full overflow-hidden rounded-full bg-line"
    >
      {indeterminate ? (
        <div className="anim-indeterminate absolute inset-y-0 w-2/5 rounded-full bg-brand" />
      ) : (
        <div className="h-full rounded-full bg-brand transition-[width] duration-300" style={{ width: `${ratio * 100}%` }} />
      )}
    </div>
  );
}

type BadgeTone = 'neutral' | 'ok' | 'warn' | 'danger' | 'brand' | 'info';

const BADGE: Record<BadgeTone, string> = {
  neutral: 'bg-sunken text-muted',
  ok: 'bg-ok-soft text-ok',
  warn: 'bg-warn-soft text-warn',
  danger: 'bg-danger-soft text-danger',
  brand: 'bg-brand-soft text-action',
  info: 'bg-info-soft text-info',
};

/** สถานะทุกแบบมีทั้งไอคอน/ข้อความ ไม่สื่อด้วยสีอย่างเดียว */
export function StatusBadge({ tone = 'neutral', icon, children, className }: { tone?: BadgeTone; icon?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <span className={cn('inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-[12.5px] font-semibold whitespace-nowrap', BADGE[tone], className)}>
      {icon && <span className="[&>svg]:size-3.5">{icon}</span>}
      {children}
    </span>
  );
}
