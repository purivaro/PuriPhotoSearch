import { useRef, type ReactNode } from 'react';
import { Dialog as RadixDialog } from 'radix-ui';
import { X } from 'lucide-react';
import { cn } from '@/lib/cn';
import { Button } from './Button';

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  /** ความกว้างสูงสุด */
  size?: 'sm' | 'md' | 'lg' | 'xl';
  /** กันปิดด้วยการคลิกนอกกล่อง/Esc ระหว่างที่มีงานค้าง */
  dismissible?: boolean;
}

const WIDTH = { sm: 'max-w-[420px]', md: 'max-w-[540px]', lg: 'max-w-[720px]', xl: 'max-w-[920px]' };

export function Dialog({ open, onOpenChange, title, description, children, footer, size = 'md', dismissible = true }: DialogProps) {
  const contentRef = useRef<HTMLDivElement>(null);
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="anim-fade fixed inset-0 z-[70] bg-scrim" />
        <RadixDialog.Content
          ref={contentRef}
          onOpenAutoFocus={(event) => {
            // ช่องที่ติด data-autofocus ได้ focus ก่อนปุ่มปิด แม้เปิดซ้อนบน dialog อื่น
            const target = contentRef.current?.querySelector<HTMLElement>('[data-autofocus]');
            if (target) {
              event.preventDefault();
              target.focus();
            }
          }}
          onEscapeKeyDown={(e) => !dismissible && e.preventDefault()}
          onPointerDownOutside={(e) => !dismissible && e.preventDefault()}
          onInteractOutside={(e) => !dismissible && e.preventDefault()}
          className={cn(
            'anim-pop fixed top-1/2 left-1/2 z-[71] flex max-h-[calc(100vh-48px)] w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl bg-surface shadow-pop outline-none',
            WIDTH[size],
          )}
        >
          <div className="flex items-start gap-3 px-6 pt-5 pb-3">
            <div className="min-w-0 flex-1">
              <RadixDialog.Title className="text-lg font-bold text-ink">{title}</RadixDialog.Title>
              {description ? (
                <RadixDialog.Description className="mt-1 text-sm text-muted">{description}</RadixDialog.Description>
              ) : (
                <RadixDialog.Description className="sr-only">{title}</RadixDialog.Description>
              )}
            </div>
            {dismissible && (
              <RadixDialog.Close
                aria-label="ปิด"
                className="-mt-1 -mr-2 inline-flex size-9 shrink-0 items-center justify-center rounded-lg text-muted hover:bg-sunken hover:text-ink"
              >
                <X className="size-5" aria-hidden />
              </RadixDialog.Close>
            )}
          </div>
          {children !== undefined && <div className="scroll-thin min-h-0 flex-1 overflow-y-auto px-6 pb-5">{children}</div>}
          {footer && <div className="flex flex-wrap items-center justify-end gap-2.5 border-t border-line px-6 py-4">{footer}</div>}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  tone?: 'default' | 'danger';
  busy?: boolean;
  onConfirm: () => void;
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  cancelLabel = 'ยกเลิก',
  tone = 'default',
  busy,
  onConfirm,
}: ConfirmDialogProps) {
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description={description}
      size="sm"
      dismissible={!busy}
      footer={
        <>
          <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
            {cancelLabel}
          </Button>
          <Button variant={tone === 'danger' ? 'danger' : 'primary'} onClick={onConfirm} busy={busy}>
            {confirmLabel}
          </Button>
        </>
      }
    />
  );
}

interface SheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** ชื่อสำหรับ screen reader */
  label: string;
  children: ReactNode;
  onCloseAutoFocus?: (event: Event) => void;
}

/** แผงเลื่อนจากขวา ใช้เมื่อพื้นที่กลางแคบเกินกว่าจะวาง inspector ด้านข้าง */
export function Sheet({ open, onOpenChange, label, children, onCloseAutoFocus }: SheetProps) {
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="anim-fade fixed inset-0 z-[60] bg-scrim" />
        <RadixDialog.Content
          onCloseAutoFocus={onCloseAutoFocus}
          className="anim-slide fixed top-0 right-0 bottom-0 z-[61] flex w-[380px] max-w-[calc(100vw-40px)] flex-col bg-surface shadow-pop outline-none"
        >
          <RadixDialog.Title className="sr-only">{label}</RadixDialog.Title>
          <RadixDialog.Description className="sr-only">{label}</RadixDialog.Description>
          {children}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
