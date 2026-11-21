import { useEffect, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Info, SlidersHorizontal, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { EmptyState, ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { Id, SelectionSpec } from '@/lib/ipc/types';
import { IDENTITY } from './editGeometry';
import { presetSummary, usePresets } from './EditPresets';

interface ApplyPresetDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selection: SelectionSpec;
  /** จำนวนรูปที่ผู้ใช้เห็นว่าเลือกไว้ */
  count: number;
}

/** ใช้ preset กับรูปที่เลือกทั้งหมดในครั้งเดียว: แทนค่าแสงและสี ไม่แตะการตัด/หมุนของแต่ละรูป (DECISIONS ข้อ 20) */
export function ApplyPresetDialog({ open, onOpenChange, selection, count }: ApplyPresetDialogProps) {
  const qc = useQueryClient();
  const presets = usePresets();
  const list = presets.data ?? [];
  const [chosen, setChosen] = useState<Id | null>(null);
  const preset = list.find((p) => p.id === chosen) ?? null;

  useEffect(() => {
    if (open) setChosen(null);
  }, [open]);

  const apply = useMutation({
    mutationFn: () => call('apply_photo_edits', { selection, settings: { ...IDENTITY, adjust: preset!.adjust }, parts: { geometry: false, light: true, color: true, masks: false } }),
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ['photos'] });
      void qc.invalidateQueries({ queryKey: ['photo'] });
      const skipped = result.notReady > 0 ? ` · ข้าม ${formatCount(result.notReady)} รูปที่ยังเตรียมข้อมูลไม่เสร็จ` : '';
      toast.success(
        result.updated > 0
          ? `ใช้ “${preset?.name}” กับ ${formatCount(result.updated)} รูปแล้ว ไฟล์ต้นฉบับไม่ถูกแตะ${skipped}`
          : `ทุกรูปใช้ “${preset?.name}” อยู่แล้ว${skipped}`,
      );
      onOpenChange(false);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="ใช้ preset"
      description={`กับ ${formatCount(count)} รูปที่เลือก`}
      dismissible={!apply.isPending}
      footer={
        <>
          <Button onClick={() => onOpenChange(false)} disabled={apply.isPending}>
            ยกเลิก
          </Button>
          <Button
            variant="primary"
            icon={<SlidersHorizontal className="size-4" aria-hidden />}
            disabled={!preset}
            busy={apply.isPending}
            onClick={() => apply.mutate()}
            data-testid="apply-preset-confirm"
          >
            ใช้กับ {formatCount(count)} รูป
          </Button>
        </>
      }
    >
      {presets.isPending ? (
        <LoadingBlock />
      ) : presets.isError ? (
        <ErrorState compact error={presets.error} onRetry={() => void presets.refetch()} />
      ) : list.length === 0 ? (
        <EmptyState
          compact
          icon={<SlidersHorizontal aria-hidden />}
          title="ยังไม่มี preset"
          description="เปิดรูปแล้วกด “แต่งภาพ” → แท็บ Preset เพื่อบันทึกค่าที่ปรับไว้ หรือนำเข้า preset .xmp จาก Lightroom"
        />
      ) : (
        <div className="space-y-3" data-testid="apply-preset">
          <div role="radiogroup" aria-label="preset" className="space-y-1.5">
            {list.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={chosen === p.id}
                onClick={() => setChosen(p.id)}
                className={cn(
                  'flex w-full items-start gap-2.5 rounded-xl border px-3.5 py-2.5 text-left',
                  chosen === p.id ? 'border-action bg-brand-soft' : 'border-line hover:bg-sunken',
                )}
                data-testid="apply-preset-option"
              >
                <span
                  aria-hidden
                  className={cn('mt-1 flex size-4 shrink-0 items-center justify-center rounded-full border-2', chosen === p.id ? 'border-action' : 'border-line-strong')}
                >
                  {chosen === p.id && <span className="size-2 rounded-full bg-action" />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5">
                    <span className="truncate text-sm font-semibold text-ink">{p.name}</span>
                    <span className="shrink-0 rounded bg-sunken px-1.5 py-px text-[12px] font-semibold text-muted">{p.source === 'lightroom' ? 'Lightroom' : 'ของฉัน'}</span>
                  </span>
                  <span className="mt-0.5 block text-[12.5px] text-muted">{presetSummary(p.adjust)}</span>
                  {p.unsupported.length > 0 && (
                    <span className="mt-0.5 flex items-start gap-1 text-[12.5px] text-warn">
                      <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden />
                      นำเข้าไม่ได้: {p.unsupported.join(' · ')}
                    </span>
                  )}
                </span>
              </button>
            ))}
          </div>
          <p className="flex items-start gap-1.5 text-[13px] text-muted">
            <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
            แทนค่าแสงและสีของทุกรูปที่เลือก การตัดและหมุนของแต่ละรูปคงเดิม · เก็บในแอป ไฟล์ต้นฉบับไม่ถูกแตะ ย้อนได้ด้วย “กลับเป็นภาพต้นฉบับ”
          </p>
        </div>
      )}
    </Dialog>
  );
}
