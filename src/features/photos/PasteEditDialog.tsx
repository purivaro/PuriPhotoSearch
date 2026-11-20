import { useEffect, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ClipboardPaste, Info, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { InlineNotice } from '@/components/ui/Feedback';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { EditParts, SelectionSpec } from '@/lib/ipc/types';
import { useEditClipboard } from '@/stores/editClipboard';
import { describeEdit } from './editSummary';

interface PasteEditDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selection: SelectionSpec;
  /** จำนวนรูปที่ผู้ใช้เห็นว่าเลือกไว้ */
  count: number;
}

/** วางการแต่งที่คัดลอกไว้ให้รูปที่เลือก โดยเลือกได้ว่าจะวางส่วนไหน (แบบ Paste Settings ของ Lightroom) */
export function PasteEditDialog({ open, onOpenChange, selection, count }: PasteEditDialogProps) {
  const qc = useQueryClient();
  const copied = useEditClipboard((s) => s.copied);
  const summary = copied ? describeEdit(copied.settings) : null;
  // ค่าเริ่มต้นแบบ Lightroom: วางแสงและสี ไม่วางการตัด/หมุน (แต่ละรูปมักตัดต่างกัน)
  const [parts, setParts] = useState<EditParts>({ geometry: false, light: true, color: true });

  useEffect(() => {
    if (open) setParts({ geometry: false, light: Boolean(summary?.light) || !summary?.color, color: Boolean(summary?.color) || !summary?.light });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const apply = useMutation({
    mutationFn: () => call('apply_photo_edits', { selection, settings: copied!.settings, parts }),
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ['photos'] });
      void qc.invalidateQueries({ queryKey: ['photo'] });
      const skipped = result.notReady > 0 ? ` · ข้าม ${formatCount(result.notReady)} รูปที่ยังเตรียมข้อมูลไม่เสร็จ` : '';
      toast.success(
        result.updated > 0 ? `วางการแต่งให้ ${formatCount(result.updated)} รูปแล้ว ไฟล์ต้นฉบับไม่ถูกแตะ${skipped}` : `ทุกรูปมีการแต่งนี้อยู่แล้ว${skipped}`,
      );
      onOpenChange(false);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const rows: { key: keyof EditParts; label: string; detail: string | null }[] = [
    { key: 'light', label: 'แสง', detail: summary?.light ?? null },
    { key: 'color', label: 'สี', detail: summary?.color ?? null },
    { key: 'geometry', label: 'ตัด หมุน และปรับเอียง', detail: summary?.geometry ?? null },
  ];
  const nothing = !parts.geometry && !parts.light && !parts.color;

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="วางการแต่ง"
      description={copied ? `จาก ${copied.filename} ไปยัง ${formatCount(count)} รูปที่เลือก` : undefined}
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>
          <Button
            variant="primary"
            icon={<ClipboardPaste className="size-4" aria-hidden />}
            disabled={!copied || nothing}
            busy={apply.isPending}
            onClick={() => apply.mutate()}
            data-testid="paste-edit-confirm"
          >
            วางให้ {formatCount(count)} รูป
          </Button>
        </>
      }
    >
      {!copied ? (
        <p className="text-sm text-muted">ยังไม่ได้คัดลอกการแต่ง เปิดรูปที่แต่งไว้แล้วเลือก “คัดลอกการแต่ง” จากเมนู ⋮</p>
      ) : (
        <div className="space-y-3" data-testid="paste-edit">
          <p className="text-sm font-semibold text-ink">เลือกส่วนที่จะวาง</p>
          <div className="space-y-2">
            {rows.map((row) => (
              <label key={row.key} className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-line px-3.5 py-2.5" data-testid={`paste-part-${row.key}`}>
                <input
                  type="checkbox"
                  className="mt-0.5 size-4 shrink-0 accent-[var(--action)]"
                  checked={parts[row.key]}
                  onChange={(e) => setParts((p) => ({ ...p, [row.key]: e.target.checked }))}
                />
                <span className="min-w-0 text-sm">
                  <span className="font-semibold text-ink">{row.label}</span>
                  <span className="block text-[13px] text-muted">{row.detail ?? 'รูปต้นทางไม่ได้แต่งส่วนนี้ — ถ้าเลือก ส่วนนี้ของรูปที่เลือกจะถูกล้างเป็นค่าเริ่มต้น'}</span>
                </span>
              </label>
            ))}
          </div>
          {parts.geometry && (
            <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
              กรอบตัดเดียวกันจะใช้กับทุกรูป ถ้าแต่ละรูปจัดองค์ประกอบต่างกัน อาจตัดคนหรือของสำคัญออก (ใบหน้าและชื่อยังอยู่ครบ)
            </InlineNotice>
          )}
          <p className="flex items-start gap-1.5 text-[13px] text-muted">
            <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
            ส่วนที่ไม่ได้เลือกของแต่ละรูปคงเดิม · เก็บในแอป ไฟล์ต้นฉบับไม่ถูกแตะ ย้อนได้ด้วย “กลับเป็นภาพต้นฉบับ”
          </p>
        </div>
      )}
    </Dialog>
  );
}
